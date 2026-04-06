use std::{io, path::PathBuf, sync::Arc, time::Duration, time::Instant};

use anyhow::Result;

use super::super::image_pipeline::{
    PreparedImagePayload, is_always_upload_mode, load_encoded_payload, load_image_data,
    load_image_dimensions, load_payload_hash, load_rgba_frame, prefetch_neighbors,
    should_decode_rgba_frame,
};
use super::super::render::{FrameRenderInput, RenderOptions, render_frame};
use super::super::state::ViewerState;
use super::RenderModeFlags;

/// 現在のモードに応じた描画を行う関数
pub(super) fn render_current_mode(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: usize,
    viewport: &super::super::state::Viewport,
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
        load_payload_hash(
            current_index,
            &image_files[current_index],
            image_data.as_ref(),
            image_dimensions,
            state,
        )
    };
    let encoded_payload = load_encoded_payload(image_data.as_ref());
    let rgba_frame = if should_decode_rgba_frame(image_dimensions, state.image_diff_mode()) {
        load_rgba_frame(current_index, image_data.as_ref(), state)
    } else {
        None
    };
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
            skip_image: false,
            preserve_image: false,
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
            source_dimensions: image_dimensions,
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
            status_message: None,
            processing_duration,
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
pub(super) fn render_prepared_mode(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: usize,
    viewport: &super::super::state::Viewport,
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
            skip_image: false,
            preserve_image: false,
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
            source_dimensions: prepared.source_dimensions,
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
            status_message: None,
            processing_duration: prepared.prepare_duration,
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

/// 非同期準備中の待機画面を描画する関数
pub(super) fn render_pending_mode(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: usize,
    viewport: &super::super::state::Viewport,
    state: &mut ViewerState,
    message: Option<&str>,
    flags: RenderModeFlags,
) -> Result<()> {
    let sidebar_entries = state
        .sidebar_tree
        .render_entries(image_files.get(current_index));

    let _ = render_frame(
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
            skip_image: true,
            preserve_image: true,
            sidebar_visible: state.sidebar_visible(),
            header_visible: state.header_visible(),
            statusbar_visible: state.statusbar_visible(),
            sidebar_size: state.sidebar_size(),
            header_bg_color: state.header_bg_color(),
            header_fg_color: state.header_fg_color(),
            statusbar_bg_color: state.statusbar_bg_color(),
            statusbar_fg_color: state.statusbar_fg_color(),
            always_upload: false,
            transport_mode: state.transport_mode(),
            diff_mode: state.image_diff_mode(),
            image_dimensions: (0, 0),
            source_dimensions: (0, 0),
            payload_hash: 0,
            image_data: Arc::<[u8]>::from([]),
            encoded_payload: Arc::<str>::from(""),
            dirty_ratio: state.dirty_ratio(),
            tile_grid: state.tile_grid(),
            skip_step: state.skip_step(),
            zoom_factor: state.zoom_factor(),
            pan_x: state.pan_x(),
            pan_y: state.pan_y(),
            rgba_frame: None,
            overlay_visible: state.overlay_visible(),
            status_message: message.map(str::to_owned),
            processing_duration: Duration::ZERO,
        },
        &mut state.image_render_state,
    )?;

    Ok(())
}
