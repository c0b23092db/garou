use crossterm::terminal::size;
use image::{RgbaImage, imageops::FilterType};

const CELL_PIXEL_WIDTH: u32 = 8;
const CELL_PIXEL_HEIGHT: u32 = 16;
const DPI_SCALE_NUM: u32 = 1;
const DPI_SCALE_DEN: u32 = 1;
const PIXEL_SAFETY_FACTOR: u32 = 2;

/// ターミナルの物理ピクセル制限を計算する関数
pub(in crate::tui) fn terminal_pixel_limit() -> (u32, u32) {
    let (cols, rows) = size().unwrap_or((120, 40));
    let content_rows = rows.saturating_sub(2).max(1);

    let width_px = (u32::from(cols)
        .saturating_mul(CELL_PIXEL_WIDTH)
        .saturating_mul(DPI_SCALE_NUM)
        / DPI_SCALE_DEN)
        .saturating_mul(PIXEL_SAFETY_FACTOR)
        .max(1);
    let height_px = (u32::from(content_rows)
        .saturating_mul(CELL_PIXEL_HEIGHT)
        .saturating_mul(DPI_SCALE_NUM)
        / DPI_SCALE_DEN)
        .saturating_mul(PIXEL_SAFETY_FACTOR)
        .max(1);

    (width_px, height_px)
}

/// RGBA画像がターミナルの物理ピクセル制限を超える場合にリサイズする関数
pub(in crate::tui) fn resize_rgba_if_needed(image: RgbaImage, max_w: u32, max_h: u32) -> RgbaImage {
    let (w, h) = image.dimensions();
    if w <= max_w && h <= max_h {
        return image;
    }

    let scale_w = max_w as f32 / w as f32;
    let scale_h = max_h as f32 / h as f32;
    let scale = scale_w.min(scale_h).max(0.01);
    let target_w = ((w as f32 * scale).round() as u32).max(1);
    let target_h = ((h as f32 * scale).round() as u32).max(1);

    image::imageops::resize(&image, target_w, target_h, FilterType::Triangle)
}
