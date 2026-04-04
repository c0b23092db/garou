use crate::core::SortField;
use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};
use std::{
    io,
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

mod debounce;
mod image_pipeline;
mod input;
mod render;
mod runtime;
mod state;

use image_pipeline::{
    PreparedImagePayload, is_always_upload_mode, load_encoded_payload, load_image_data,
    load_image_dimensions, load_payload_hash, load_rgba_frame, prefetch_neighbors,
    prepare_image_payload,
};
use input::{process_key, process_mouse};
use render::{
    FrameRenderInput, HeaderRenderInput, RenderOptions, filetree::SidebarTree,
    image::ImageRenderState, render_frame, render_header_only,
};
use runtime::ImageCache;
use state::{CacheState, ImageProcessingConfig, PerformanceStats, PreviewState, UiState, Viewport};
use state::{NavDirection, RedrawMode, ViewerState};

pub use state::ConfigOption;

#[derive(Debug, Clone, Copy)]
struct RenderModeFlags {
    refresh_image: bool,
    full_refresh: bool,
    prefetch_after: bool,
}

#[derive(Debug, Clone)]
struct PreviewRequest {
    index: usize,
    generation: u64,
    path: PathBuf,
    diff_mode: crate::model::config::ImageDiffMode,
}

#[derive(Debug)]
struct PreviewResponse {
    index: usize,
    generation: u64,
    payload: Result<PreparedImagePayload, String>,
}

/// ビューワーを実行する関数
pub fn run_viewer(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    start_index: usize,
    options: ConfigOption,
) -> Result<()> {
    if image_files.is_empty() {
        return Ok(());
    }

    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture, Hide)?;

    let mut current_index = start_index;
    let result = viewer_loop(stdout, image_files, &mut current_index, options);

    execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
    disable_raw_mode()?;

    result
}

/// ビューワーのメインループを実行する関数
fn viewer_loop(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: &mut usize,
    options: ConfigOption,
) -> Result<()> {
    let mut image_files = image_files.to_vec();
    let (preview_req_tx, preview_resp_rx, preview_join) = spawn_preview_worker();
    let (initial_width, initial_height) = size()?;
    let mut viewport = Viewport {
        width: initial_width,
        height: initial_height,
    };
    let mut redraw_mode = RedrawMode::HeaderRefresh;
    let debounce_duration = Duration::from_millis(options.preview_debounce);
    let idle_poll_interval = Duration::from_millis(options.poll_interval);
    let idle_prefetch_interval = Duration::from_millis(options.prefetch_interval);
    let mut sort_field = SortField::Natural;
    let mut sort_descending = false;
    let mut pending_preview_payload: Option<(usize, u64, PreparedImagePayload)> = None;
    let mut state = ViewerState {
        pending_replace: false,
        pending_deadline: None,
        ui_state: UiState {
            sidebar_visible: options.sidebar_visible,
            sidebar_size: options.sidebar_size,
            header_visible: options.header_visible,
            statusbar_visible: options.statusbar_visible,
            overlay_visible: false,
            header_bg_color: options.header_bg_color,
            header_fg_color: options.header_fg_color,
            statusbar_bg_color: options.statusbar_bg_color,
            statusbar_fg_color: options.statusbar_fg_color,
        },
        cache: CacheState {
            image_cache: ImageCache::new(options.cache_lru_size, options.cache_max_bytes),
            image_dimensions_cache: std::collections::HashMap::new(),
            payload_hash_cache: std::collections::HashMap::new(),
        },
        preview: PreviewState {
            prefetch_size: options.prefetch_size,
            last_prefetch_state: None,
            preview_generation: 0,
            expected_preview_generation: None,
            last_idle_prefetch_at: None,
        },
        image_config: ImageProcessingConfig {
            image_diff_mode: options.image_diff_mode,
            transport_mode: options.transport_mode,
            dirty_ratio: options.dirty_ratio.clamp(0.0, 1.0),
            tile_grid: options.tile_grid.max(1),
            skip_step: options.skip_step,
            zoom_factor: 1.0,
            pan_x: 0,
            pan_y: 0,
        },
        perf: PerformanceStats::default(),
        sidebar_tree: SidebarTree::from_image_files(
            &image_files,
            *current_index,
            &options.image_extensions,
        ),
        image_render_state: ImageRenderState::new(),
        last_nav_direction: NavDirection::Forward,
    };

    // 初回表示はワーカーで画像準備してから差し替えることで、大画像時のUI停止を抑える。
    let initial_diff_mode = state.image_diff_mode();
    submit_preview_request(
        &preview_req_tx,
        &image_files,
        *current_index,
        &mut state,
        initial_diff_mode,
    );

    loop {
        while let Ok(response) = preview_resp_rx.try_recv() {
            if state.expected_preview_generation() != Some((response.index, response.generation)) {
                continue;
            }

            state.set_expected_preview_generation(None);
            if let Ok(payload) = response.payload
                && response.index == *current_index
            {
                pending_preview_payload = Some((response.index, response.generation, payload));
                redraw_mode = RedrawMode::ImageReplace;
            }
        }

        match redraw_mode {
            RedrawMode::Idle => {}
            RedrawMode::HeaderRefresh => {
                if state.overlay_visible() {
                    render_current_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        RenderModeFlags {
                            refresh_image: false,
                            full_refresh: false,
                            prefetch_after: false,
                        },
                    )?;
                    redraw_mode = RedrawMode::Idle;
                    continue;
                }

                let sidebar_entries = state
                    .sidebar_tree
                    .render_entries(image_files.get(*current_index));
                render_header_only(
                    stdout,
                    HeaderRenderInput {
                        image_files: &image_files,
                        current_index: *current_index,
                        sidebar_entries: &sidebar_entries,
                        term_width: viewport.width,
                        term_height: viewport.height,
                        sidebar_visible: state.sidebar_visible(),
                        header_visible: state.header_visible(),
                        sidebar_size: state.sidebar_size(),
                        header_bg_color: state.header_bg_color(),
                        header_fg_color: state.header_fg_color(),
                    },
                )?;
                redraw_mode = RedrawMode::Idle;
            }
            RedrawMode::FullRefresh => {
                render_current_mode(
                    stdout,
                    &image_files,
                    *current_index,
                    &viewport,
                    &mut state,
                    RenderModeFlags {
                        refresh_image: true,
                        full_refresh: true,
                        prefetch_after: false,
                    },
                )?;
                redraw_mode = RedrawMode::Idle;
            }
            RedrawMode::LayoutRefresh => {
                render_current_mode(
                    stdout,
                    &image_files,
                    *current_index,
                    &viewport,
                    &mut state,
                    RenderModeFlags {
                        refresh_image: true,
                        full_refresh: true,
                        prefetch_after: false,
                    },
                )?;
                redraw_mode = RedrawMode::Idle;
            }
            RedrawMode::ImageRefresh => {
                render_current_mode(
                    stdout,
                    &image_files,
                    *current_index,
                    &viewport,
                    &mut state,
                    RenderModeFlags {
                        refresh_image: true,
                        full_refresh: false,
                        prefetch_after: false,
                    },
                )?;
                redraw_mode = RedrawMode::Idle;
            }
            RedrawMode::ImageReplace => {
                if let Some((payload_index, payload_generation, payload)) =
                    pending_preview_payload.take()
                    && payload_index == *current_index
                    && payload_generation == state.preview_generation()
                {
                    render_prepared_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        payload,
                        RenderModeFlags {
                            refresh_image: false,
                            full_refresh: false,
                            prefetch_after: false,
                        },
                    )?;
                }
                redraw_mode = RedrawMode::Idle;
            }
        }

        if state.pending_replace
            && let Some(deadline) = state.pending_deadline
        {
            let now = Instant::now();
            if now >= deadline {
                state.pending_replace = false;
                state.pending_deadline = None;
                let diff_mode = state.image_diff_mode();
                submit_preview_request(
                    &preview_req_tx,
                    &image_files,
                    *current_index,
                    &mut state,
                    diff_mode,
                );
                redraw_mode = RedrawMode::Idle;
                continue;
            }

            if event::poll(Duration::from_millis(0))?
                && handle_event(
                    event::read()?,
                    &mut image_files,
                    &mut sort_field,
                    &mut sort_descending,
                    current_index,
                    &mut redraw_mode,
                    &mut state,
                    &mut viewport,
                    debounce_duration,
                )?
                .0
            {
                break;
            }

            let remaining = deadline.saturating_duration_since(now);
            let nap = remaining.min(Duration::from_millis(5));
            smol::block_on(smol::Timer::after(nap));
            continue;
        }

        if event::poll(idle_poll_interval)? {
            let (should_quit, index_changed) = handle_event(
                event::read()?,
                &mut image_files,
                &mut sort_field,
                &mut sort_descending,
                current_index,
                &mut redraw_mode,
                &mut state,
                &mut viewport,
                debounce_duration,
            )?;

            if index_changed {
                if debounce_duration.is_zero() {
                    let diff_mode = state.image_diff_mode();
                    submit_preview_request(
                        &preview_req_tx,
                        &image_files,
                        *current_index,
                        &mut state,
                        diff_mode,
                    );
                }
                state.set_last_idle_prefetch_at(None);
            }

            if should_quit {
                break;
            }
        } else if redraw_mode == RedrawMode::Idle
            && !state.pending_replace
            && state.expected_preview_generation().is_none()
            && state.prefetch_size() > 0
        {
            let now = Instant::now();
            if state
                .last_idle_prefetch_at()
                .is_none_or(|last| now.duration_since(last) >= idle_prefetch_interval)
            {
                let prefetch_steps = state.prefetch_size();
                prefetch_neighbors(&image_files, *current_index, &mut state, prefetch_steps);
                state.set_last_idle_prefetch_at(Some(now));
            }
        }
    }

    drop(preview_req_tx);
    let _ = preview_join.join();

    Ok(())
}

/// 現在のモードに応じた描画を行う関数
fn render_current_mode(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: usize,
    viewport: &Viewport,
    state: &mut ViewerState,
    flags: RenderModeFlags,
) -> Result<()> {
    let processing_started = Instant::now();
    let image_data = load_image_data(image_files, current_index, state)?;
    let image_dimensions = load_image_dimensions(image_files, current_index, state)?;
    let always_upload = is_always_upload_mode(state.image_diff_mode());
    let payload_hash = if always_upload {
        0
    } else {
        load_payload_hash(current_index, image_data.as_ref(), state)
    };
    let encoded_payload = load_encoded_payload(image_data.as_ref());
    let rgba_frame = load_rgba_frame(current_index, image_data.as_ref(), state);
    let processing_duration = processing_started.elapsed();
    let sidebar_entries = state
        .sidebar_tree
        .render_entries(image_files.get(current_index));

    let frame_metrics = render_frame(
        stdout,
        FrameRenderInput {
            image_files,
            current_index,
            sidebar_entries: &sidebar_entries,
            term_width: viewport.width,
            term_height: viewport.height,
        },
        RenderOptions {
            refresh_image: flags.refresh_image,
            full_refresh: flags.full_refresh,
            sidebar_visible: state.sidebar_visible(),
            header_visible: state.header_visible(),
            statusbar_visible: state.statusbar_visible(),
            sidebar_size: state.sidebar_size(),
            header_bg_color: state.header_bg_color(),
            header_fg_color: state.header_fg_color(),
            statusbar_bg_color: state.statusbar_bg_color(),
            statusbar_fg_color: state.statusbar_fg_color(),
            always_upload,
            transport_mode: state.transport_mode(),
            diff_mode: state.image_diff_mode(),
            image_dimensions,
            payload_hash,
            image_data,
            encoded_payload,
            dirty_ratio: state.dirty_ratio(),
            tile_grid: state.tile_grid(),
            skip_step: state.skip_step(),
            zoom_factor: state.zoom_factor(),
            pan_x: state.pan_x(),
            pan_y: state.pan_y(),
            rgba_frame,
            overlay_visible: state.overlay_visible(),
            processing_duration,
            cache_hit_rate: state.cache_hit_rate(),
        },
        &mut state.image_render_state,
    )?;
    state.record_render_metrics(
        frame_metrics.render_duration,
        frame_metrics.dirty_tiles,
        frame_metrics.placement,
    );

    if flags.prefetch_after {
        let idle_prefetch_steps = state.prefetch_size();
        prefetch_neighbors(image_files, current_index, state, idle_prefetch_steps);
    }

    Ok(())
}

/// 非同期で準備済みの画像データを使って描画を行う関数
fn render_prepared_mode(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: usize,
    viewport: &Viewport,
    state: &mut ViewerState,
    prepared: PreparedImagePayload,
    flags: RenderModeFlags,
) -> Result<()> {
    let always_upload = is_always_upload_mode(state.image_diff_mode());
    let sidebar_entries = state
        .sidebar_tree
        .render_entries(image_files.get(current_index));

    if state.image_cache().enabled() {
        state
            .image_cache_mut()
            .insert(current_index, prepared.image_data.clone());
    }
    state
        .image_dimensions_cache_mut()
        .insert(current_index, prepared.image_dimensions);
    state
        .payload_hash_cache_mut()
        .insert(current_index, prepared.payload_hash);

    let frame_metrics = render_frame(
        stdout,
        FrameRenderInput {
            image_files,
            current_index,
            sidebar_entries: &sidebar_entries,
            term_width: viewport.width,
            term_height: viewport.height,
        },
        RenderOptions {
            refresh_image: flags.refresh_image,
            full_refresh: flags.full_refresh,
            sidebar_visible: state.sidebar_visible(),
            header_visible: state.header_visible(),
            statusbar_visible: state.statusbar_visible(),
            sidebar_size: state.sidebar_size(),
            header_bg_color: state.header_bg_color(),
            header_fg_color: state.header_fg_color(),
            statusbar_bg_color: state.statusbar_bg_color(),
            statusbar_fg_color: state.statusbar_fg_color(),
            always_upload,
            transport_mode: state.transport_mode(),
            diff_mode: state.image_diff_mode(),
            image_dimensions: prepared.image_dimensions,
            payload_hash: prepared.payload_hash,
            image_data: prepared.image_data,
            encoded_payload: prepared.encoded_payload,
            dirty_ratio: state.dirty_ratio(),
            tile_grid: state.tile_grid(),
            skip_step: state.skip_step(),
            zoom_factor: state.zoom_factor(),
            pan_x: state.pan_x(),
            pan_y: state.pan_y(),
            rgba_frame: prepared.rgba_frame,
            overlay_visible: state.overlay_visible(),
            processing_duration: prepared.prepare_duration,
            cache_hit_rate: state.cache_hit_rate(),
        },
        &mut state.image_render_state,
    )?;
    state.record_render_metrics(
        frame_metrics.render_duration,
        frame_metrics.dirty_tiles,
        frame_metrics.placement,
    );

    if flags.prefetch_after {
        let idle_prefetch_steps = state.prefetch_size();
        prefetch_neighbors(image_files, current_index, state, idle_prefetch_steps);
    }

    Ok(())
}

/// プレビュー準備リクエストをワーカーへ送信する
fn submit_preview_request(
    tx: &mpsc::Sender<PreviewRequest>,
    image_files: &[PathBuf],
    index: usize,
    state: &mut ViewerState,
    diff_mode: crate::model::config::ImageDiffMode,
) {
    if let Some((expected_index, _)) = state.expected_preview_generation()
        && expected_index == index
    {
        return;
    }

    if let Some(path) = image_files.get(index) {
        state.increment_preview_generation();
        let generation = state.preview_generation();
        state.set_expected_preview_generation(Some((index, generation)));
        if tx
            .send(PreviewRequest {
                index,
                generation,
                path: path.clone(),
                diff_mode,
            })
            .is_err()
        {
            state.set_expected_preview_generation(None);
        }
    }
}

/// 画像プレビュー準備用のワーカースレッドを起動する
fn spawn_preview_worker() -> (
    mpsc::Sender<PreviewRequest>,
    mpsc::Receiver<PreviewResponse>,
    thread::JoinHandle<()>,
) {
    let (req_tx, req_rx) = mpsc::channel::<PreviewRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<PreviewResponse>();

    let join = thread::spawn(move || {
        while let Ok(mut request) = req_rx.recv() {
            while let Ok(next) = req_rx.try_recv() {
                request = next;
            }

            let payload =
                prepare_image_payload(&request.path, request.diff_mode).map_err(|e| e.to_string());
            if resp_tx
                .send(PreviewResponse {
                    index: request.index,
                    generation: request.generation,
                    payload,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (req_tx, resp_rx, join)
}

/// 入力イベントを処理する関数
fn handle_event(
    event: Event,
    image_files: &mut Vec<PathBuf>,
    sort_field: &mut SortField,
    sort_descending: &mut bool,
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    viewport: &mut Viewport,
    debounce_duration: Duration,
) -> Result<(bool, bool)> {
    let previous_index = *current_index;

    let should_quit = match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => process_key(
            key,
            image_files,
            current_index,
            redraw_mode,
            state,
            debounce_duration,
            viewport.height,
            sort_field,
            sort_descending,
        ),
        Event::Mouse(mouse) => process_mouse(
            mouse,
            current_index,
            redraw_mode,
            state,
            debounce_duration,
            state.sidebar_size().max(1),
            viewport.height,
        ),
        Event::Resize(width, height) => {
            viewport.width = width;
            viewport.height = height;
            *redraw_mode = RedrawMode::LayoutRefresh;
            false
        }
        _ => false,
    };

    Ok((should_quit, *current_index != previous_index))
}

