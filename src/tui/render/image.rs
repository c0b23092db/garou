//! 画像描画の公開入口モジュール。
//! レイアウト計算・転送モード解決・プロトコル文字列組み立てを集約する。

mod difference;
mod layout;
mod protocol;
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

pub use layout::{MAX_ZOOM, MIN_ZOOM, Placement};
pub use state::{ImageRenderMetrics, ImageRenderParams, ImageRenderState, RgbaFrame};

use self::{
    difference::{dirty_ratio_from_area, extract_rect_rgba, find_dirty_tiles},
    layout::compute_placement,
    protocol::{send_patch_rgba, send_place, send_upload},
    transport::prepare_upload_payload,
};

pub use protocol::send_delete;
#[allow(unused_imports)]
pub use transport::{ResolvedTransport, UploadPayload, UploadPixelFormat, resolve_transport_mode};

pub(in crate::tui) fn decode_rgba_payload(image_data: &[u8]) -> Option<RgbaFrame> {
    difference::decode_rgba_frame(image_data)
}

fn hash_encoded_payload(encoded_payload: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    encoded_payload.hash(&mut hasher);
    hasher.finish()
}

/// セルの縦横比（高さ / 幅）
fn cell_height_to_width_ratio() -> f32 {
    let (width, height) = super::cell_pixel_size();
    height as f32 / width as f32
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
        if state.has_uploaded {
            send_delete(stdout)?;
        }
        state.reset_upload_state();
    }

    // 常時再転送モードでは比較しないため、数MBになり得る payload のハッシュ計算を省く。
    let encoded_payload_hash =
        (!params.always_upload).then(|| hash_encoded_payload(params.encoded_payload.as_ref()));
    let same_payload_as_last = !params.refresh_image
        && encoded_payload_hash.is_some()
        && state.has_uploaded
        && state.last_payload_hash == Some(params.payload_hash)
        && state.last_encoded_payload_hash == encoded_payload_hash;
    // 端末が保持する画像は常に 1 枚なので、直前に送った画像と同じときだけ転送を省ける
    let should_upload_payload = !same_payload_as_last;

    let placement = compute_placement(
        params.term_width,
        params.available_height,
        params.start_x,
        params.image_dimensions,
        params.zoom_factor,
        params.pan_center,
        cell_height_to_width_ratio(),
    );

    queue!(stdout, MoveTo(placement.x, placement.y))?;

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
                placement.image_pixels_per_cell(),
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
        state.last_encoded_payload_hash = encoded_payload_hash;
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

/// 転送済み画像の配置だけを更新する（ズーム・パン用。デコードや再転送はしない）。
/// 転送済み画像がなければ None を返す。
pub fn render_placement(
    stdout: &mut io::Stdout,
    state: &mut ImageRenderState,
    term_width: u32,
    available_height: u32,
    start_x: u16,
    zoom_factor: f32,
    pan_center: (f32, f32),
) -> Result<Option<Placement>> {
    let Some(last) = state.last_placement.filter(|_| state.has_uploaded) else {
        return Ok(None);
    };
    let placement = compute_placement(
        term_width,
        available_height,
        start_x,
        last.image,
        zoom_factor,
        pan_center,
        cell_height_to_width_ratio(),
    );
    if placement != last {
        queue!(stdout, SavePosition, MoveTo(placement.x, placement.y))?;
        send_place(stdout, placement)?;
        queue!(stdout, RestorePosition)?;
        stdout.flush()?;
        state.last_placement = Some(placement);
    }
    Ok(Some(placement))
}
