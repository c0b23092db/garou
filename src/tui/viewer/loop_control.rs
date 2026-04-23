use crate::core::SortField;
use anyhow::Result;
use crossterm::event;
use std::{
    io,
    path::PathBuf,
    sync::mpsc::Sender,
    time::{Duration, Instant},
};

use super::event_dispatch::{EventContext, handle_event};
use super::loop_support::{
    LoopRenderContext, PendingPreviewState, PreviewSubmitParams, request_preview_and_render_pending,
};
use super::worker::PreviewRequest;
use super::{RedrawMode, RenderModeFlags};
use crate::tui::state::{ViewerState, Viewport};

pub(super) enum LoopStep {
    NotHandled,
    Continue,
    Break,
}

pub(super) struct LoopPhaseContext<'a> {
    pub(super) stdout: &'a mut io::Stdout,
    pub(super) image_files: &'a mut Vec<PathBuf>,
    pub(super) sort_field: &'a mut SortField,
    pub(super) sort_descending: &'a mut bool,
    pub(super) current_index: &'a mut usize,
    pub(super) redraw_mode: &'a mut RedrawMode,
    pub(super) state: &'a mut ViewerState,
    pub(super) viewport: &'a mut Viewport,
    pub(super) debounce_duration: Duration,
    pub(super) preview_req_tx: &'a Sender<PreviewRequest>,
    pub(super) pending_preview: &'a mut PendingPreviewState,
}

pub(super) fn handle_pending_replace_phase(ctx: &mut LoopPhaseContext<'_>) -> Result<LoopStep> {
    if !(ctx.state.pending_replace && ctx.state.pending_deadline.is_some()) {
        return Ok(LoopStep::NotHandled);
    }

    let Some(deadline) = ctx.state.pending_deadline else {
        return Ok(LoopStep::NotHandled);
    };

    let now = Instant::now();
    if now >= deadline {
        ctx.state.pending_replace = false;
        ctx.state.pending_deadline = None;
        let diff_mode = ctx.state.image_config.image_diff_mode;
        let _ = request_preview_and_render_pending(
            &mut LoopRenderContext {
                stdout: ctx.stdout,
                image_files: ctx.image_files.as_slice(),
                current_index: *ctx.current_index,
                viewport: ctx.viewport,
                state: ctx.state,
            },
            ctx.preview_req_tx,
            ctx.pending_preview,
            PreviewSubmitParams {
                diff_mode,
                force_submit: false,
                flags: RenderModeFlags {
                    refresh_image: false,
                    full_refresh: false,
                    prefetch_after: false,
                },
                force_refresh_on_replace: false,
            },
        )?;
        *ctx.redraw_mode = RedrawMode::Idle;
        return Ok(LoopStep::Continue);
    }

    if event::poll(Duration::from_millis(0))?
        && handle_event(
            event::read()?,
            &mut EventContext {
                image_files: ctx.image_files,
                sort_field: ctx.sort_field,
                sort_descending: ctx.sort_descending,
                current_index: ctx.current_index,
                redraw_mode: ctx.redraw_mode,
                state: ctx.state,
                viewport: ctx.viewport,
                debounce_duration: ctx.debounce_duration,
            },
        )?
        .0
    {
        return Ok(LoopStep::Break);
    }

    let remaining = deadline.saturating_duration_since(now);
    let nap = remaining.min(Duration::from_millis(5));
    smol::block_on(smol::Timer::after(nap));
    Ok(LoopStep::Continue)
}

pub(super) fn handle_polled_event_phase(
    ctx: &mut LoopPhaseContext<'_>,
    idle_poll_interval: Duration,
) -> Result<LoopStep> {
    if !event::poll(idle_poll_interval)? {
        return Ok(LoopStep::NotHandled);
    }

    let (should_quit, index_changed) = handle_event(
        event::read()?,
        &mut EventContext {
            image_files: ctx.image_files,
            sort_field: ctx.sort_field,
            sort_descending: ctx.sort_descending,
            current_index: ctx.current_index,
            redraw_mode: ctx.redraw_mode,
            state: ctx.state,
            viewport: ctx.viewport,
            debounce_duration: ctx.debounce_duration,
        },
    )?;

    if index_changed {
        if ctx.debounce_duration.is_zero() {
            let diff_mode = ctx.state.image_config.image_diff_mode;
            let _ = request_preview_and_render_pending(
                &mut LoopRenderContext {
                    stdout: ctx.stdout,
                    image_files: ctx.image_files.as_slice(),
                    current_index: *ctx.current_index,
                    viewport: ctx.viewport,
                    state: ctx.state,
                },
                ctx.preview_req_tx,
                ctx.pending_preview,
                PreviewSubmitParams {
                    diff_mode,
                    force_submit: false,
                    flags: RenderModeFlags {
                        refresh_image: false,
                        full_refresh: false,
                        prefetch_after: false,
                    },
                    force_refresh_on_replace: false,
                },
            )?;
        }
        ctx.state.preview.last_idle_prefetch_at = None;
    }

    if should_quit {
        return Ok(LoopStep::Break);
    }

    Ok(LoopStep::Continue)
}

pub(super) fn maybe_run_idle_prefetch(
    image_files: &[PathBuf],
    current_index: usize,
    state: &mut ViewerState,
    redraw_mode: RedrawMode,
    idle_prefetch_interval: Duration,
) {
    use super::super::image_pipeline::prefetch_neighbors;

    if redraw_mode != RedrawMode::Idle
        || state.pending_replace
        || state.preview.expected_preview_generation.is_some()
        || state.preview.prefetch_size == 0
    {
        return;
    }

    let now = Instant::now();
    if state
        .preview
        .last_idle_prefetch_at
        .is_some_and(|last| now.duration_since(last) < idle_prefetch_interval)
    {
        return;
    }

    let prefetch_steps = state.preview.prefetch_size;
    prefetch_neighbors(image_files, current_index, state, prefetch_steps);
    state.preview.last_idle_prefetch_at = Some(now);
}
