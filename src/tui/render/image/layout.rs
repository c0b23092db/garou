//! 画像を端末セルへ収めるレイアウト計算を担う

/// 画像配置情報 (start_x, display_width_cells, display_height_cells)
pub type Placement = (u16, u32, u32);

/// 画像の描画セル数を計算する
pub fn compute_placement(
    term_width: u32,
    available_height: u32,
    start_x: u16,
    image_dimensions: (u32, u32),
) -> Placement {
    let (img_width, img_height) = image_dimensions;

    let max_display_height = available_height.max(1);
    let max_display_width = term_width.saturating_sub(2).max(1);

    let cell_height_to_width_ratio = 2.0;
    let image_aspect = img_width as f32 / img_height as f32;

    let width_limit_by_height =
        (max_display_height as f32 * image_aspect * cell_height_to_width_ratio) as u32;
    let display_width_cells = max_display_width.min(width_limit_by_height).max(1);
    let mut display_height_cells =
        (display_width_cells as f32 / (image_aspect * cell_height_to_width_ratio)) as u32;
    display_height_cells = display_height_cells.max(1).min(max_display_height);

    (start_x, display_width_cells, display_height_cells)
}
