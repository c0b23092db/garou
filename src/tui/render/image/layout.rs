//! 画像を端末セルへ収めるレイアウト計算を担う

/// 画像配置情報 (start_x, display_width_cells, display_height_cells)
pub type Placement = (u16, u32, u32);

/// 画像の描画セル数を計算する
pub fn compute_placement(
    term_width: u32,
    available_height: u32,
    start_x: u16,
    image_dimensions: (u32, u32),
    zoom_factor: f32,
) -> Placement {
    let (img_width, img_height) = image_dimensions;

    let max_display_height = available_height.max(1);
    let max_display_width = term_width.saturating_sub(2).max(1);

    let cell_height_to_width_ratio = 2.0;
    let image_aspect = img_width as f32 / img_height as f32;

    let width_limit_by_height =
        (max_display_height as f32 * image_aspect * cell_height_to_width_ratio) as u32;
    let fit_width_cells = max_display_width.min(width_limit_by_height).max(1);
    let mut fit_height_cells =
        (fit_width_cells as f32 / (image_aspect * cell_height_to_width_ratio)) as u32;
    fit_height_cells = fit_height_cells.max(1).min(max_display_height);

    let zoom = zoom_factor.clamp(0.1, 4.0);
    let max_zoom_width = max_display_width.saturating_mul(4).max(1);
    let max_zoom_height = max_display_height.saturating_mul(4).max(1);

    let mut display_width_cells = (fit_width_cells as f32 * zoom).round() as u32;
    let mut display_height_cells = (fit_height_cells as f32 * zoom).round() as u32;
    display_width_cells = display_width_cells.clamp(1, max_zoom_width);
    display_height_cells = display_height_cells.clamp(1, max_zoom_height);

    (start_x, display_width_cells, display_height_cells)
}
