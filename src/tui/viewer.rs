use crate::core::SortField;
use anyhow::Result;
use crossterm::{
    cursor::{Hide, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};
use std::{io, path::PathBuf, sync::mpsc::Sender, time::Duration};

use super::ConfigOption;
use super::render::image::ImageRenderState;
use super::render::{HeaderRenderInput, filetree::SidebarTree};
use super::runtime::ImageCache;
use super::state::{
    CacheState, ImageProcessingConfig, NavDirection, PerformanceStats, PreviewState, RedrawMode,
    UiState, ViewerState, Viewport,
};

mod event_dispatch;
mod loop_control;
mod loop_support;
mod render;
mod worker;

use loop_control::LoopStep;
use loop_support::PendingPreviewState;
use worker::{PreviewRequest, spawn_preview_worker};

#[derive(Debug, Clone, Copy)]
struct RenderModeFlags {
    refresh_image: bool,
    full_refresh: bool,
}

impl RenderModeFlags {
    /// 差分更新
    const NONE: Self = Self {
        refresh_image: false,
        full_refresh: false,
    };
    /// 画像を再転送する
    const IMAGE: Self = Self {
        refresh_image: true,
        full_refresh: false,
    };
    /// 画面を消して画像も再転送する
    const FULL: Self = Self {
        refresh_image: true,
        full_refresh: true,
    };
}

/// メインループが扱う状態一式
struct Viewer<'a> {
    stdout: &'a mut io::Stdout,
    image_files: Vec<PathBuf>,
    current_index: usize,
    viewport: Viewport,
    redraw_mode: RedrawMode,
    state: ViewerState,
    sort_field: SortField,
    sort_descending: bool,
    pending_preview: PendingPreviewState,
    preview_req_tx: Sender<PreviewRequest>,
    debounce_duration: Duration,
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
    super::render::detect_cell_pixel_size(stdout)?;

    let result = viewer_loop(stdout, image_files.to_vec(), start_index, options);

    execute!(stdout, Show, DisableMouseCapture, LeaveAlternateScreen)?;
    disable_raw_mode()?;

    result
}

/// ビューワーのメインループを実行する関数
fn viewer_loop(
    stdout: &mut io::Stdout,
    image_files: Vec<PathBuf>,
    current_index: usize,
    options: ConfigOption,
) -> Result<()> {
    let (preview_req_tx, preview_resp_rx, preview_join) = spawn_preview_worker();
    let (width, height) = size()?;
    let idle_poll_interval = Duration::from_millis(options.poll_interval);
    let idle_prefetch_interval = Duration::from_millis(options.prefetch_interval);
    let state = ViewerState {
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
            prepared: ImageCache::new(options.cache_lru_size, options.cache_max_bytes),
            prefetch_inflight: Default::default(),
            prepared_epoch: 0,
        },
        preview: PreviewState {
            prefetch_size: options.prefetch_size,
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
            image_width: options.image_width,
            image_height: options.image_height,
            image_filter_type: options.image_filter_type,
            zoom_factor: 1.0,
            pan_center: (0.5, 0.5),
            render_scale: 1,
        },
        perf: PerformanceStats::default(),
        sidebar_tree: SidebarTree::from_image_files(
            &image_files,
            current_index,
            &options.image_extensions,
        ),
        image_render_state: ImageRenderState::new(),
        last_nav_direction: NavDirection::Forward,
        keymap: options.keymap,
        drag_origin: None,
        prompt: None,
    };
    let mut v = Viewer {
        stdout,
        image_files,
        current_index,
        viewport: Viewport { width, height },
        redraw_mode: RedrawMode::HeaderRefresh,
        state,
        sort_field: SortField::Natural,
        sort_descending: false,
        pending_preview: PendingPreviewState::default(),
        preview_req_tx,
        debounce_duration: Duration::from_millis(options.preview_debounce),
    };

    v.request_preview(false, RenderModeFlags::NONE)?;

    loop {
        v.drain_preview_responses(&preview_resp_rx)?;
        v.maybe_render_loading_indicator()?;
        v.redraw()?;

        match v.pending_replace_phase()? {
            LoopStep::Break => break,
            LoopStep::Continue => continue,
            LoopStep::NotHandled => {}
        }

        match v.polled_event_phase(idle_poll_interval)? {
            LoopStep::Break => break,
            LoopStep::Continue => {}
            LoopStep::NotHandled => v.maybe_run_idle_prefetch(idle_prefetch_interval),
        }
    }

    drop(v);
    let _ = preview_join.join();

    Ok(())
}

impl Viewer<'_> {
    /// redraw_mode に応じて描画し、Idle に戻す
    fn redraw(&mut self) -> Result<()> {
        let mode = std::mem::replace(&mut self.redraw_mode, RedrawMode::Idle);
        match mode {
            RedrawMode::Idle => {}
            RedrawMode::HeaderRefresh => {
                let sidebar_entries = self
                    .state
                    .sidebar_tree
                    .render_entries(self.image_files.get(self.current_index));
                let ui = &self.state.ui_state;
                super::render::render_header_only(
                    self.stdout,
                    HeaderRenderInput {
                        image_files: &self.image_files,
                        current_index: self.current_index,
                        sidebar_entries: &sidebar_entries,
                        term_width: self.viewport.width,
                        term_height: self.viewport.height,
                        sidebar_visible: ui.sidebar_visible,
                        header_visible: ui.header_visible,
                        sidebar_size: ui.sidebar_size,
                        header_bg_color: ui.header_bg_color,
                        header_fg_color: ui.header_fg_color,
                    },
                )?;
            }
            RedrawMode::FullRefresh | RedrawMode::LayoutRefresh => {
                self.handle_refresh_redraw(RenderModeFlags::FULL)?
            }
            RedrawMode::ImageRefresh => self.handle_refresh_redraw(RenderModeFlags::IMAGE)?,
            RedrawMode::ImagePlace => {
                let placement = super::render::render_image_placement(
                    self.stdout,
                    (self.viewport.width, self.viewport.height),
                    self.state.ui_state.sidebar_visible,
                    self.state.ui_state.sidebar_size,
                    self.state.image_config.zoom_factor,
                    self.state.image_config.pan_center,
                    &mut self.state.image_render_state,
                )?;
                // 転送済み画像がなければ通常の再描画に任せる
                match placement {
                    Some(placement) => self.state.perf.last_image_rect = Some(placement),
                    None => self.redraw_mode = RedrawMode::ImageRefresh,
                }
            }
            RedrawMode::ImageReplace => self.handle_image_replace_redraw()?,
        }
        // 入力欄はステータスバーの位置に重ねる
        if mode != RedrawMode::Idle
            && let Some(prompt) = &self.state.prompt
        {
            let ui = &self.state.ui_state;
            super::render::statusbar::render_statusbar(
                self.stdout,
                u32::from(self.viewport.width),
                u32::from(self.viewport.height),
                super::render::statusbar::StatusbarContent {
                    elapsed: Duration::ZERO,
                    source_dimensions: (0, 0),
                    rendered_dimensions: (0, 0),
                    status_message: Some(
                        &prompt.text(
                            self.image_files
                                .get(self.current_index)
                                .map(PathBuf::as_path),
                        ),
                    ),
                },
                ui.statusbar_bg_color,
                ui.statusbar_fg_color,
            )?;
            io::Write::flush(self.stdout)?;
        }
        Ok(())
    }
}
