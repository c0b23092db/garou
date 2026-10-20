//! 画像を端末セルへ収めるレイアウト計算を担う

pub const MIN_ZOOM: f32 = 0.1;
pub const MAX_ZOOM: f32 = 8.0;

/// 画像配置情報
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Placement {
    /// 配置先の列
    pub x: u16,
    /// 配置先の行
    pub y: u16,
    /// 表示セル幅
    pub cols: u32,
    /// 表示セル高さ
    pub rows: u32,
    /// 表示する画像内の矩形 (x, y, w, h) [px]
    pub crop: (u32, u32, u32, u32),
    /// 転送済み画像のサイズ [px]
    pub image: (u32, u32),
}

impl Placement {
    /// 切り出しが画像全体か
    pub fn is_full_crop(&self) -> bool {
        self.crop == (0, 0, self.image.0, self.image.1)
    }

    /// 切り出し範囲の中心（画像に対する 0.0..=1.0 の正規化座標）
    pub fn center(&self) -> (f32, f32) {
        let (x, y, w, h) = self.crop;
        (
            (x as f32 + w as f32 / 2.0) / self.image.0.max(1) as f32,
            (y as f32 + h as f32 / 2.0) / self.image.1.max(1) as f32,
        )
    }

    /// 画像のうち表示されている割合
    pub fn visible(&self) -> (f32, f32) {
        (
            self.crop.2 as f32 / self.image.0.max(1) as f32,
            self.crop.3 as f32 / self.image.1.max(1) as f32,
        )
    }

    /// 1 セルに表示される画像のピクセル数 (幅, 高さ)
    pub fn image_pixels_per_cell(&self) -> (u32, u32) {
        let (_, _, w, h) = self.crop;
        ((w / self.cols.max(1)).max(1), (h / self.rows.max(1)).max(1))
    }

    /// 画面セルに対応する画像上の点（正規化座標）。画像外なら None
    pub fn image_point(&self, col: u16, row: u16) -> Option<(f32, f32)> {
        let dx = u32::from(col).checked_sub(u32::from(self.x))?;
        let dy = u32::from(row).checked_sub(u32::from(self.y))?;
        if dx >= self.cols || dy >= self.rows {
            return None;
        }
        let (x, y, w, h) = self.crop;
        let fx = (dx as f32 + 0.5) / self.cols as f32;
        let fy = (dy as f32 + 0.5) / self.rows as f32;
        Some((
            (x as f32 + fx * w as f32) / self.image.0.max(1) as f32,
            (y as f32 + fy * h as f32) / self.image.1.max(1) as f32,
        ))
    }
}

/// 表示割合 `visible` と中心 `center` から、画像の一軸上の切り出し範囲 (開始, 長さ) を求める
fn crop_axis(len: u32, visible: f32, center: f32) -> (u32, u32) {
    let visible = visible.clamp(0.0, 1.0);
    let size = ((len as f32 * visible).round() as u32).clamp(1, len);
    let start = ((center - visible / 2.0) * len as f32)
        .round()
        .clamp(0.0, (len - size) as f32) as u32;
    (start, size)
}

/// 画像の描画セル数と切り出し範囲を計算する。
/// 拡大で表示領域を超える分は画面外にはみ出させず、`center` を中心に切り出す。
// ponytail: 切り出し元は端末サイズに縮小済みの転送画像なので、大きな画像を高倍率にすると粗くなる。
// 必要になったら、ズーム確定後（デバウンス後）に倍率分の解像度で再準備・再転送する。
pub fn compute_placement(
    term_width: u32,
    available_height: u32,
    start_x: u16,
    image_dimensions: (u32, u32),
    zoom_factor: f32,
    center: (f32, f32),
    cell_height_to_width_ratio: f32,
) -> Placement {
    let img_width = image_dimensions.0.max(1);
    let img_height = image_dimensions.1.max(1);

    let max_display_height = available_height.max(1);
    let max_display_width = term_width.saturating_sub(2).max(1);

    let image_aspect = img_width as f32 / img_height as f32;

    let width_limit_by_height =
        (max_display_height as f32 * image_aspect * cell_height_to_width_ratio) as u32;
    let fit_width_cells = max_display_width.min(width_limit_by_height).max(1);
    let fit_height_cells = ((fit_width_cells as f32 / (image_aspect * cell_height_to_width_ratio))
        as u32)
        .clamp(1, max_display_height);

    let zoom = zoom_factor.clamp(MIN_ZOOM, MAX_ZOOM);
    let zoomed_width = fit_width_cells as f32 * zoom;
    let zoomed_height = fit_height_cells as f32 * zoom;
    let cols = (zoomed_width.round() as u32).clamp(1, max_display_width);
    let rows = (zoomed_height.round() as u32).clamp(1, max_display_height);

    let (crop_x, crop_w) = crop_axis(img_width, cols as f32 / zoomed_width, center.0);
    let (crop_y, crop_h) = crop_axis(img_height, rows as f32 / zoomed_height, center.1);

    Placement {
        x: start_x,
        y: 1,
        cols,
        rows,
        crop: (crop_x, crop_y, crop_w, crop_h),
        image: (img_width, img_height),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CENTER: (f32, f32) = (0.5, 0.5);

    #[test]
    fn compute_placement_fits_to_available_space() {
        let p = compute_placement(120, 40, 3, (400, 200), 1.0, CENTER, 2.0);

        assert_eq!((p.x, p.y, p.cols, p.rows), (3, 1, 118, 29));
        assert!(p.is_full_crop());
    }

    #[test]
    fn compute_placement_clamps_to_minimum_size() {
        let p = compute_placement(1, 0, 0, (100, 100), 0.01, CENTER, 2.0);

        assert_eq!((p.x, p.y, p.cols, p.rows), (0, 1, 1, 1));
    }

    #[test]
    fn zoom_crops_around_center_and_stays_inside_image() {
        // 正方形画像 (fit = 80x40 セル) を 2 倍 → 表示領域いっぱいで中央半分を切り出す
        let p = compute_placement(82, 40, 0, (400, 400), 2.0, CENTER, 2.0);
        assert_eq!((p.cols, p.rows), (80, 40));
        assert_eq!(p.crop, (100, 100, 200, 200));
        assert_eq!(p.center(), (0.5, 0.5));

        // 中心が端を越えても切り出しは画像内に収まる
        let p = compute_placement(82, 40, 0, (400, 400), 2.0, (1.0, 0.0), 2.0);
        assert_eq!(p.crop, (200, 0, 200, 200));
    }

    #[test]
    fn compute_placement_uses_cell_ratio() {
        // セル 10x20 (比 2.0) と 10x25 (比 2.5) で、正方形画像の行数が比に応じて変わる
        let p = compute_placement(102, 100, 0, (400, 400), 1.0, CENTER, 2.0);
        assert_eq!((p.cols, p.rows), (100, 50));
        let p = compute_placement(102, 100, 0, (400, 400), 1.0, CENTER, 2.5);
        assert_eq!((p.cols, p.rows), (100, 40));
    }

    #[test]
    fn image_point_maps_screen_cell_to_image() {
        let p = compute_placement(82, 40, 5, (400, 400), 2.0, CENTER, 2.0);
        let (u, v) = p.image_point(5, 1).unwrap();
        assert!((u - 0.25).abs() < 0.01 && (v - 0.25).abs() < 0.01);
        assert_eq!(p.image_point(4, 1), None);
        // 2 倍ズームで 200px を 80 列・40 行に表示 → 1 セル 2x5 px
        assert_eq!(p.image_pixels_per_cell(), (2, 5));
    }
}
