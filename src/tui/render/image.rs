//! 画像描画の公開入口モジュール。
//! レイアウト計算・転送モード解決・プロトコル文字列組み立てを集約する。

mod difference;
mod layout;
mod protocol;
#[allow(dead_code)]
mod resize;
mod state;
mod transport;

use crate::model::config::ImageDiffMode;
use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use crossterm::{
    cursor::{MoveTo, RestorePosition, SavePosition},
    queue,
};
use std::{
    collections::hash_map::DefaultHasher,
    hash::{Hash, Hasher},
    io::{self, Write},
    time::Instant,
};

pub use state::{ImageRenderMetrics, ImageRenderParams, ImageRenderState, RgbaFrame};

use self::{
    difference::{dirty_ratio_from_area, extract_rect_rgba, find_dirty_tiles},
    layout::compute_placement,
    protocol::{send_patch_rgba, send_place, send_upload},
    transport::prepare_upload_payload,
};

pub use protocol::send_delete;
pub use transport::{
    ResolvedTransport, UploadPayload, prepare_upload_payload_offthread, resolve_transport_mode,
};

/// 画像の内容からハッシュ値を計算する。
pub(in crate::tui) fn hash_image_payload(image_data: &[u8], diff_mode: ImageDiffMode) -> u64 {
    let mut hasher = DefaultHasher::new();
    match diff_mode {
        ImageDiffMode::All => {
            // All: 常時再アップロード運用。ハッシュ値は判定に使わない。
            0u8.hash(&mut hasher);
        }
        ImageDiffMode::Full => {
            image_data.hash(&mut hasher);
        }
        ImageDiffMode::Half => {
            image_data.len().hash(&mut hasher);
            for chunk in image_data.chunks(6) {
                for idx in [0usize, 2, 4] {
                    if let Some(byte) = chunk.get(idx) {
                        byte.hash(&mut hasher);
                    }
                }
            }
        }
    }
    hasher.finish()
}

pub(in crate::tui) fn decode_rgba_payload(image_data: &[u8]) -> Option<RgbaFrame> {
    difference::decode_rgba_frame(image_data)
}

/// 画像をターミナルに描画する。
pub fn render_image(
    stdout: &mut io::Stdout,
    state: &mut ImageRenderState,
    params: ImageRenderParams,
) -> Result<ImageRenderMetrics> {
    let render_started_at = Instant::now();
    queue!(stdout, SavePosition)?;
    let mut dirty_tiles: Option<usize> = None;

    if params.refresh_image {
        send_delete(stdout)?;
        state.has_uploaded = false;
        state.last_payload_hash = None;
        state.last_placement = None;
        state.last_rgba_frame = None;
    }

    let should_upload_payload = params.always_upload
        || !state.has_uploaded
        || state.last_payload_hash != Some(params.payload_hash);

    let placement = compute_placement(
        params.term_width,
        params.available_height,
        params.start_x,
        params.image_dimensions,
        params.zoom_factor,
        params.pan_x,
        params.pan_y,
    );

    queue!(stdout, MoveTo(placement.0, placement.1))?;

    let mut upload_completed = false;

    if should_upload_payload {
        let mut patched = false;
        if !matches!(params.diff_mode, ImageDiffMode::All)
            && state.has_uploaded
            && !params.refresh_image
            && let (Some(prev), Some(next)) =
                (state.last_rgba_frame.as_ref(), params.rgba_frame.clone())
            && let Some(rects) = find_dirty_tiles(
                prev,
                &next,
                params.diff_mode,
                params.tile_grid,
                params.skip_step,
            )
        {
            dirty_tiles = Some(rects.len());
            if rects.is_empty() {
                patched = true;
                state.last_rgba_frame = Some(next);
            } else {
                let dirty_area = rects.iter().fold(0u32, |acc, rect| {
                    acc.saturating_add(rect.width.saturating_mul(rect.height))
                });
                if dirty_ratio_from_area(&next, dirty_area) <= params.dirty_ratio {
                    for rect in &rects {
                        let patch_bytes = extract_rect_rgba(&next, *rect);
                        let patch_payload = general_purpose::STANDARD.encode(patch_bytes);
                        send_patch_rgba(
                            stdout,
                            rect.x,
                            rect.y,
                            rect.width,
                            rect.height,
                            &patch_payload,
                        )?;
                    }
                    stdout.flush()?;
                    patched = true;
                    upload_completed = true;
                    state.last_rgba_frame = Some(next);
                }
            }
        }

        if !patched {
            state.last_rgba_frame = if matches!(params.diff_mode, ImageDiffMode::All) {
                None
            } else {
                params.rgba_frame.clone()
            };

            let upload_payload = if let Some(prepared) = params.prepared_upload_payload.clone() {
                prepared
            } else {
                let requested = resolve_transport_mode(params.transport_mode);
                prepare_upload_payload(
                    requested,
                    &params.encoded_payload,
                    params.image_data.as_ref(),
                    &mut state.shared_memory,
                )
            };
            send_upload(stdout, placement, &upload_payload)?;
            stdout.flush()?;
            upload_completed = true;
        }

        state.has_uploaded = true;
        state.last_payload_hash = Some(params.payload_hash);
        state.last_placement = Some(placement);
    } else if state.last_placement != Some(placement) {
        send_place(stdout, placement)?;
        state.last_placement = Some(placement);
    }

    if upload_completed && state.last_placement != Some(placement) {
        send_place(stdout, placement)?;
        state.last_placement = Some(placement);
    }

    queue!(stdout, RestorePosition)?;
    Ok(ImageRenderMetrics {
        render_duration: Instant::now().duration_since(render_started_at),
        dirty_tiles,
        placement,
    })
}
