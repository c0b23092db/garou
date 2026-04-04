use crate::core::SortField;
use crate::model::config::TransportMode;
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
    time::{Duration, Instant},
};

use super::ConfigOption;
use super::input::{process_key, process_mouse};
use super::state::{RedrawMode, ViewerState, Viewport};

mod render;
mod worker;

use render::{render_current_mode, render_pending_mode, render_prepared_mode};
use worker::{spawn_preview_worker, submit_preview_request};

#[derive(Debug, Clone, Copy)]
struct RenderModeFlags {
    refresh_image: bool,
    full_refresh: bool,
    prefetch_after: bool,
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
    use super::image_pipeline::prefetch_neighbors;
    use super::render::image::ImageRenderState;
    use super::render::{HeaderRenderInput, filetree::SidebarTree};
    use super::runtime::ImageCache;
    use super::state::{
        CacheState, ImageProcessingConfig, NavDirection, PerformanceStats, PreviewState,
    };

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
    let mut pending_preview_payload: Option<(
        usize,
        u64,
        super::image_pipeline::PreparedImagePayload,
    )> = None;
    let mut pending_preview_started_at: Option<Instant> = None;
    let mut pending_loading_rendered = false;
    let mut state = ViewerState {
        pending_replace: false,
        pending_deadline: None,
        ui_state: super::state::UiState {
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

    let initial_diff_mode = state.image_diff_mode();
    if submit_preview_request(
        &preview_req_tx,
        &image_files,
        *current_index,
        &mut state,
        initial_diff_mode,
        false,
    )
    {
        pending_preview_started_at = Some(Instant::now());
        pending_loading_rendered = false;
        render_pending_mode(
            stdout,
            &image_files,
            *current_index,
            &viewport,
            &mut state,
            None,
            RenderModeFlags {
                refresh_image: false,
                full_refresh: false,
                prefetch_after: false,
            },
        )?;
    }

    loop {
        while let Ok(response) = preview_resp_rx.try_recv() {
            if state.expected_preview_generation() != Some((response.index, response.generation)) {
                continue;
            }

            state.set_expected_preview_generation(None);
            pending_preview_started_at = None;
            pending_loading_rendered = false;
            match response.payload {
                Ok(payload) if response.index == *current_index => {
                    pending_preview_payload = Some((response.index, response.generation, payload));
                    redraw_mode = RedrawMode::ImageReplace;
                }
                Err(error) if response.index == *current_index =>
                {
                    render_pending_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        Some(&error),
                        RenderModeFlags {
                            refresh_image: false,
                            full_refresh: false,
                            prefetch_after: false,
                        },
                    )?;
                    redraw_mode = RedrawMode::Idle;
                }
                _ => {}
            }
        }

        if state.expected_preview_generation().is_some()
            && pending_preview_payload.is_none()
            && pending_preview_started_at.is_some()
            && !pending_loading_rendered
            && pending_preview_started_at
                .is_some_and(|started| started.elapsed() >= Duration::from_millis(100))
        {
            render_pending_mode(
                stdout,
                &image_files,
                *current_index,
                &viewport,
                &mut state,
                Some("Loading images across time"),
                RenderModeFlags {
                    refresh_image: false,
                    full_refresh: false,
                    prefetch_after: false,
                },
            )?;
            pending_loading_rendered = true;
        }

        match redraw_mode {
            RedrawMode::Idle => {}
            RedrawMode::HeaderRefresh => {
                if state.overlay_visible() && state.transport_mode() != TransportMode::File {
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
                super::render::render_header_only(
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
                let diff_mode = state.image_diff_mode();
                if submit_preview_request(
                    &preview_req_tx,
                    &image_files,
                    *current_index,
                    &mut state,
                    diff_mode,
                    true,
                ) {
                    pending_preview_started_at = Some(Instant::now());
                    pending_loading_rendered = false;
                    render_pending_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        None,
                        RenderModeFlags {
                            refresh_image: true,
                            full_refresh: true,
                            prefetch_after: false,
                        },
                    )?;
                } else {
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
                }
                redraw_mode = RedrawMode::Idle;
            }
            RedrawMode::LayoutRefresh => {
                let diff_mode = state.image_diff_mode();
                if submit_preview_request(
                    &preview_req_tx,
                    &image_files,
                    *current_index,
                    &mut state,
                    diff_mode,
                    true,
                ) {
                    pending_preview_started_at = Some(Instant::now());
                    pending_loading_rendered = false;
                    render_pending_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        None,
                        RenderModeFlags {
                            refresh_image: true,
                            full_refresh: true,
                            prefetch_after: false,
                        },
                    )?;
                } else {
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
                }
                redraw_mode = RedrawMode::Idle;
            }
            RedrawMode::ImageRefresh => {
                let diff_mode = state.image_diff_mode();
                if submit_preview_request(
                    &preview_req_tx,
                    &image_files,
                    *current_index,
                    &mut state,
                    diff_mode,
                    true,
                ) {
                    pending_preview_started_at = Some(Instant::now());
                    pending_loading_rendered = false;
                    render_pending_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        None,
                        RenderModeFlags {
                            refresh_image: true,
                            full_refresh: false,
                            prefetch_after: false,
                        },
                    )?;
                } else {
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
                }
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
                if submit_preview_request(
                    &preview_req_tx,
                    &image_files,
                    *current_index,
                    &mut state,
                    diff_mode,
                    false,
                ) {
                    pending_preview_started_at = Some(Instant::now());
                    pending_loading_rendered = false;
                    render_pending_mode(
                        stdout,
                        &image_files,
                        *current_index,
                        &viewport,
                        &mut state,
                        None,
                        RenderModeFlags {
                            refresh_image: false,
                            full_refresh: false,
                            prefetch_after: false,
                        },
                    )?;
                }
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
                    if submit_preview_request(
                        &preview_req_tx,
                        &image_files,
                        *current_index,
                        &mut state,
                        diff_mode,
                        false,
                    ) {
                        pending_preview_started_at = Some(Instant::now());
                        pending_loading_rendered = false;
                        render_pending_mode(
                            stdout,
                            &image_files,
                            *current_index,
                            &viewport,
                            &mut state,
                            None,
                            RenderModeFlags {
                                refresh_image: false,
                                full_refresh: false,
                                prefetch_after: false,
                            },
                        )?;
                    }
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
