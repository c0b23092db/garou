//! RGBA フレーム差分の判定と矩形抽出を担う

use crate::model::config::ImageDiffMode;

use super::state::RgbaFrame;

/// 差分が存在する最小外接矩形
#[derive(Debug, Clone, Copy)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// 画像のRGBデータを比較して差分があるかどうかを返す
fn is_pixel_changed(
    prev_pixels: &[u8],
    next_pixels: &[u8],
    idx: usize,
    diff_mode: ImageDiffMode,
) -> bool {
    match diff_mode {
        ImageDiffMode::All => false,
        // Full: RGB の全番地を比較する（A は判定に使わない）
        ImageDiffMode::Full => {
            prev_pixels[idx] != next_pixels[idx]
                || prev_pixels[idx + 1] != next_pixels[idx + 1]
                || prev_pixels[idx + 2] != next_pixels[idx + 2]
        }
        // Half: RGB の 0,2,4... 相当になるよう R/B を比較する
        ImageDiffMode::Half => {
            prev_pixels[idx] != next_pixels[idx] || prev_pixels[idx + 2] != next_pixels[idx + 2]
        }
    }
}

/// 画像データを RGBA フレームへデコードする
pub fn decode_rgba_frame(image_data: &[u8]) -> Option<RgbaFrame> {
    let dyn_img = ::image::load_from_memory(image_data).ok()?;
    let rgba = dyn_img.to_rgba8();
    Some(RgbaFrame {
        width: rgba.width(),
        height: rgba.height(),
        pixels: std::sync::Arc::from(rgba.into_raw()),
    })
}

/// 2 つの RGBA フレームを比較して差分矩形を返す
#[allow(dead_code)]
pub fn find_dirty_rect(
    prev: &RgbaFrame,
    next: &RgbaFrame,
    diff_mode: ImageDiffMode,
) -> Option<DirtyRect> {
    if prev.width != next.width || prev.height != next.height {
        return None;
    }

    if matches!(diff_mode, ImageDiffMode::All) {
        return None;
    }

    let width = prev.width as usize;
    let height = prev.height as usize;
    let prev_pixels = prev.pixels.as_ref();
    let next_pixels = next.pixels.as_ref();

    let mut min_x = width;
    let mut min_y = height;
    let mut max_x = 0usize;
    let mut max_y = 0usize;
    let mut any_changed = false;

    for y in 0..height {
        let row_base = y * width * 4;
        for x in 0..width {
            let idx = row_base + x * 4;
            if is_pixel_changed(prev_pixels, next_pixels, idx, diff_mode) {
                any_changed = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }

    if !any_changed {
        return Some(DirtyRect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        });
    }

    Some(DirtyRect {
        x: min_x as u32,
        y: min_y as u32,
        width: (max_x - min_x + 1) as u32,
        height: (max_y - min_y + 1) as u32,
    })
}

/// 2 つの RGBA フレームをタイル分割で比較し、変化したタイル矩形を返す
pub fn find_dirty_tiles(
    prev: &RgbaFrame,
    next: &RgbaFrame,
    diff_mode: ImageDiffMode,
    tile_grid: u32,
    skip_step: u32,
) -> Option<Vec<DirtyRect>> {
    if prev.width != next.width || prev.height != next.height {
        return None;
    }

    if matches!(diff_mode, ImageDiffMode::All) {
        return None;
    }

    let width = next.width as usize;
    let height = next.height as usize;
    let tile = tile_grid.max(1) as usize;
    let sample_step = skip_step.max(1) as usize;
    let prev_pixels = prev.pixels.as_ref();
    let next_pixels = next.pixels.as_ref();

    let mut dirty_tiles = Vec::new();

    for tile_y in (0..height).step_by(tile) {
        let rect_h = (height - tile_y).min(tile);
        for tile_x in (0..width).step_by(tile) {
            let rect_w = (width - tile_x).min(tile);

            let mut changed = false;
            'scan: for y in (tile_y..(tile_y + rect_h)).step_by(sample_step) {
                let row_base = y * width * 4;
                for x in (tile_x..(tile_x + rect_w)).step_by(sample_step) {
                    let idx = row_base + x * 4;
                    if is_pixel_changed(prev_pixels, next_pixels, idx, diff_mode) {
                        changed = true;
                        break 'scan;
                    }
                }
            }

            if changed {
                dirty_tiles.push(DirtyRect {
                    x: tile_x as u32,
                    y: tile_y as u32,
                    width: rect_w as u32,
                    height: rect_h as u32,
                });
            }
        }
    }

    Some(dirty_tiles)
}

/// 差分矩形から RGBA ピクセル列を切り出す
pub fn extract_rect_rgba(frame: &RgbaFrame, rect: DirtyRect) -> Vec<u8> {
    let stride = frame.width as usize * 4;
    let row_bytes = rect.width as usize * 4;
    let mut out = Vec::with_capacity(rect.height as usize * row_bytes);
    for y in rect.y..(rect.y + rect.height) {
        let start = y as usize * stride + rect.x as usize * 4;
        out.extend_from_slice(&frame.pixels[start..start + row_bytes]);
    }
    out
}

/// 差分面積の比率を返す
pub fn dirty_ratio_from_area(frame: &RgbaFrame, dirty_area: u32) -> f32 {
    let full_area = frame.width.saturating_mul(frame.height).max(1);
    dirty_area as f32 / full_area as f32
}
