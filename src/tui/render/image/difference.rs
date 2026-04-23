//! RGBA フレーム差分の判定と矩形抽出を担う

use crate::model::config::ImageDiffMode;

use super::state::RgbaFrame;

/// 差分が存在する最小外接矩形
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DirtyRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// セルの横幅のピクセル数
const CELL_PIXEL_WIDTH: u32 = 8;
/// セルの高さのピクセル数
const CELL_PIXEL_HEIGHT: u32 = 16;

#[inline]
fn align_down(value: u32, unit: u32) -> u32 {
    (value / unit) * unit
}

/// 2つのRGBAフレームを比較して、差分が存在する最小外接矩形を返す
#[inline]
fn align_up(value: u32, unit: u32) -> u32 {
    value.saturating_add(unit.saturating_sub(1)) / unit * unit
}

/// 与えられた矩形をセルグリッドに合わせて拡大する
fn align_rect_to_cell_grid(rect: DirtyRect, frame_width: u32, frame_height: u32) -> DirtyRect {
    let mut start_x = rect.x;
    let mut end_x = rect.x.saturating_add(rect.width).min(frame_width);
    let mut start_y = rect.y;
    let mut end_y = rect.y.saturating_add(rect.height).min(frame_height);

    if frame_width > CELL_PIXEL_WIDTH {
        start_x = align_down(start_x, CELL_PIXEL_WIDTH);
        end_x = align_up(end_x, CELL_PIXEL_WIDTH).min(frame_width);
    }

    if frame_height > CELL_PIXEL_HEIGHT {
        start_y = align_down(start_y, CELL_PIXEL_HEIGHT);
        end_y = align_up(end_y, CELL_PIXEL_HEIGHT).min(frame_height);
    }

    DirtyRect {
        x: start_x,
        y: start_y,
        width: end_x.saturating_sub(start_x).max(1),
        height: end_y.saturating_sub(start_y).max(1),
    }
}

#[inline]
fn pixel_u32(bytes: &[u8], idx: usize) -> u32 {
    u32::from_ne_bytes([bytes[idx], bytes[idx + 1], bytes[idx + 2], bytes[idx + 3]])
}

/// 画像のRGBAチャンクを比較して差分があるかどうかを返す
fn is_pixel_changed(
    prev_pixels: &[u8],
    next_pixels: &[u8],
    idx: usize,
    diff_mode: ImageDiffMode,
) -> bool {
    let prev_pixel = pixel_u32(prev_pixels, idx);
    let next_pixel = pixel_u32(next_pixels, idx);

    #[cfg(target_endian = "little")]
    const RGB_MASK: u32 = 0x00FF_FFFF;
    #[cfg(target_endian = "big")]
    const RGB_MASK: u32 = 0xFFFF_FF00;

    match diff_mode {
        ImageDiffMode::All => false,
        // Full: RGBA を 4 バイト単位で読み、A を除いた RGB だけを比較する
        ImageDiffMode::Full => (prev_pixel ^ next_pixel) & RGB_MASK != 0,
        // Half: RGB の 0,2,4... 相当になるよう R/B を比較する
        ImageDiffMode::Half => (prev_pixel ^ next_pixel) & (RGB_MASK & 0x00FF_00FF) != 0,
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

    let row_bytes = width * 4;
    for y in 0..height {
        let row_base = y * row_bytes;
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
    let row_bytes = width * 4;

    for tile_y in (0..height).step_by(tile) {
        let rect_h = (height - tile_y).min(tile);
        for tile_x in (0..width).step_by(tile) {
            let rect_w = (width - tile_x).min(tile);

            let mut changed = false;
            'scan: for y in (tile_y..(tile_y + rect_h)).step_by(sample_step) {
                let row_base = y * row_bytes;
                for x in (tile_x..(tile_x + rect_w)).step_by(sample_step) {
                    let idx = row_base + x * 4;
                    if is_pixel_changed(prev_pixels, next_pixels, idx, diff_mode) {
                        changed = true;
                        break 'scan;
                    }
                }
            }

            if changed {
                let rect = align_rect_to_cell_grid(
                    DirtyRect {
                        x: tile_x as u32,
                        y: tile_y as u32,
                        width: rect_w as u32,
                        height: rect_h as u32,
                    },
                    next.width,
                    next.height,
                );
                if !dirty_tiles.contains(&rect) {
                    dirty_tiles.push(rect);
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn rgba_frame(width: u32, height: u32, pixels: &[u8]) -> RgbaFrame {
        assert_eq!(pixels.len(), (width * height * 4) as usize);

        RgbaFrame {
            width,
            height,
            pixels: Arc::from(pixels.to_vec()),
        }
    }

    #[test]
    fn find_dirty_rect_returns_zero_rect_when_frames_match() {
        let frame = rgba_frame(
            2,
            2,
            &[
                0, 0, 0, 255, 10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255,
            ],
        );

        let dirty_rect = find_dirty_rect(&frame, &frame, ImageDiffMode::Full).unwrap();

        assert_eq!(dirty_rect.x, 0);
        assert_eq!(dirty_rect.y, 0);
        assert_eq!(dirty_rect.width, 0);
        assert_eq!(dirty_rect.height, 0);
    }

    #[test]
    fn find_dirty_rect_detects_single_changed_pixel() {
        let prev = rgba_frame(
            2,
            2,
            &[
                0, 0, 0, 255, 10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255,
            ],
        );
        let next = rgba_frame(
            2,
            2,
            &[
                0, 0, 0, 255, 11, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255,
            ],
        );

        let dirty_rect = find_dirty_rect(&prev, &next, ImageDiffMode::Full).unwrap();

        assert_eq!(dirty_rect.x, 1);
        assert_eq!(dirty_rect.y, 0);
        assert_eq!(dirty_rect.width, 1);
        assert_eq!(dirty_rect.height, 1);
    }

    #[test]
    fn find_dirty_tiles_returns_only_changed_tile() {
        let prev = rgba_frame(
            4,
            4,
            &[
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
            ],
        );
        let next = rgba_frame(
            4,
            4,
            &[
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255,
                0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 1, 0, 0, 255,
            ],
        );

        let dirty_tiles = find_dirty_tiles(&prev, &next, ImageDiffMode::Full, 2, 1).unwrap();

        assert_eq!(dirty_tiles.len(), 1);
        assert_eq!(dirty_tiles[0].x, 2);
        assert_eq!(dirty_tiles[0].y, 2);
        assert_eq!(dirty_tiles[0].width, 2);
        assert_eq!(dirty_tiles[0].height, 2);
    }

    #[test]
    fn find_dirty_rect_ignores_alpha_changes_in_full_mode() {
        let prev = rgba_frame(1, 1, &[10, 20, 30, 40]);
        let next = rgba_frame(1, 1, &[10, 20, 30, 250]);

        let dirty_rect = find_dirty_rect(&prev, &next, ImageDiffMode::Full).unwrap();

        assert_eq!(dirty_rect.x, 0);
        assert_eq!(dirty_rect.y, 0);
        assert_eq!(dirty_rect.width, 0);
        assert_eq!(dirty_rect.height, 0);
    }

    #[test]
    fn find_dirty_tiles_aligns_rect_to_cell_grid() {
        let prev_pixels = vec![0u8; 32 * 32 * 4];
        let mut next_pixels = prev_pixels.clone();

        let px = 17usize;
        let py = 20usize;
        let idx = (py * 32 + px) * 4;
        next_pixels[idx] = 1;

        let prev = rgba_frame(32, 32, &prev_pixels);
        let next = rgba_frame(32, 32, &next_pixels);

        let dirty_tiles = find_dirty_tiles(&prev, &next, ImageDiffMode::Full, 10, 1).unwrap();

        assert_eq!(dirty_tiles.len(), 1);
        assert_eq!(dirty_tiles[0].x, 8);
        assert_eq!(dirty_tiles[0].y, 16);
        assert_eq!(dirty_tiles[0].width, 16);
        assert_eq!(dirty_tiles[0].height, 16);
    }
}
