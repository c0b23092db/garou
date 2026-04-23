use anyhow::Result;
use std::{
    io,
    path::PathBuf,
    sync::mpsc::{Receiver, Sender},
    time::{Duration, Instant},
};

use super::super::state::{RedrawMode, ViewerState, Viewport};
use super::RenderModeFlags;
use super::render::{render_current_mode, render_pending_mode, render_prepared_mode};
use super::worker::{PreviewRequest, PreviewResponse, submit_preview_request};

#[derive(Default)]
pub(super) struct PendingPreviewState {
    pub(super) payload: Option<(
        usize,
        u64,
        super::super::image_pipeline::PreparedImagePayload,
    )>,
    pub(super) force_refresh: bool,
    pub(super) started_at: Option<Instant>,
    pub(super) loading_rendered: bool,
}

pub(super) struct LoopRenderContext<'a> {
    pub(super) stdout: &'a mut io::Stdout,
    pub(super) image_files: &'a [PathBuf],
    pub(super) current_index: usize,
    pub(super) viewport: &'a Viewport,
    pub(super) state: &'a mut ViewerState,
}

pub(super) struct PreviewSubmitParams {
    pub(super) diff_mode: crate::model::config::ImageDiffMode,
    pub(super) force_submit: bool,
    pub(super) flags: RenderModeFlags,
    pub(super) force_refresh_on_replace: bool,
}

pub(super) fn request_preview_and_render_pending(
    ctx: &mut LoopRenderContext<'_>,
    preview_req_tx: &Sender<PreviewRequest>,
    pending_preview: &mut PendingPreviewState,
    params: PreviewSubmitParams,
) -> Result<bool> {
    if submit_preview_request(
        preview_req_tx,
        ctx.image_files,
        ctx.current_index,
        ctx.state,
        params.diff_mode,
        params.force_submit,
    ) {
        pending_preview.started_at = Some(Instant::now());
        pending_preview.loading_rendered = false;
        pending_preview.force_refresh = params.force_refresh_on_replace;
        render_pending_mode(
            ctx.stdout,
            ctx.image_files,
            ctx.current_index,
            ctx.viewport,
            ctx.state,
            None,
            params.flags,
        )?;
        return Ok(true);
    }

    Ok(false)
}

pub(super) fn drain_preview_responses(
    ctx: &mut LoopRenderContext<'_>,
    preview_resp_rx: &Receiver<PreviewResponse>,
    redraw_mode: &mut RedrawMode,
    pending_preview: &mut PendingPreviewState,
) -> Result<()> {
    while let Ok(response) = preview_resp_rx.try_recv() {
        if ctx.state.preview.expected_preview_generation
            != Some((response.index, response.generation))
        {
            continue;
        }

        ctx.state.preview.expected_preview_generation = None;
        pending_preview.started_at = None;
        pending_preview.loading_rendered = false;
        match response.payload {
            Ok(payload) if response.index == ctx.current_index => {
                pending_preview.payload = Some((response.index, response.generation, payload));
                *redraw_mode = RedrawMode::ImageReplace;
            }
            Err(error) if response.index == ctx.current_index => {
                pending_preview.force_refresh = false;
                render_pending_mode(
                    ctx.stdout,
                    ctx.image_files,
                    ctx.current_index,
                    ctx.viewport,
                    ctx.state,
                    Some(&error),
                    RenderModeFlags {
                        refresh_image: false,
                        full_refresh: false,
                        prefetch_after: false,
                    },
                )?;
                *redraw_mode = RedrawMode::Idle;
            }
            _ => {}
        }
    }

    Ok(())
}

pub(super) fn maybe_render_loading_indicator(
    ctx: &mut LoopRenderContext<'_>,
    pending_preview: &mut PendingPreviewState,
) -> Result<()> {
    if ctx.state.preview.expected_preview_generation.is_some()
        && pending_preview.payload.is_none()
        && pending_preview.started_at.is_some()
        && !pending_preview.loading_rendered
        && pending_preview
            .started_at
            .is_some_and(|started| started.elapsed() >= Duration::from_millis(100))
    {
        render_pending_mode(
            ctx.stdout,
            ctx.image_files,
            ctx.current_index,
            ctx.viewport,
            ctx.state,
            Some("Load image access time"),
            RenderModeFlags {
                refresh_image: false,
                full_refresh: false,
                prefetch_after: false,
            },
        )?;
        pending_preview.loading_rendered = true;
    }

    Ok(())
}

pub(super) fn handle_refresh_redraw(
    ctx: &mut LoopRenderContext<'_>,
    preview_req_tx: &Sender<PreviewRequest>,
    flags: RenderModeFlags,
    pending_preview: &mut PendingPreviewState,
) -> Result<()> {
    let diff_mode = ctx.state.image_config.image_diff_mode;
    let submitted = request_preview_and_render_pending(
        ctx,
        preview_req_tx,
        pending_preview,
        PreviewSubmitParams {
            diff_mode,
            force_submit: true,
            flags,
            force_refresh_on_replace: true,
        },
    )?;

    if !submitted {
        render_current_mode(
            ctx.stdout,
            ctx.image_files,
            ctx.current_index,
            ctx.viewport,
            ctx.state,
            flags,
        )?;
    }

    Ok(())
}

pub(super) fn handle_image_replace_redraw(
    ctx: &mut LoopRenderContext<'_>,
    pending_preview: &mut PendingPreviewState,
) -> Result<()> {
    if let Some((payload_index, payload_generation, payload)) = pending_preview.payload.take()
        && payload_index == ctx.current_index
        && payload_generation == ctx.state.preview.preview_generation
    {
        render_prepared_mode(
            ctx.stdout,
            ctx.image_files,
            ctx.current_index,
            ctx.viewport,
            ctx.state,
            payload,
            RenderModeFlags {
                refresh_image: pending_preview.force_refresh,
                full_refresh: false,
                prefetch_after: false,
            },
        )?;
    }

    pending_preview.force_refresh = false;
    Ok(())
}
