use std::time::{Duration, Instant};

use super::super::{
    render::image::{MAX_ZOOM, MIN_ZOOM},
    state::{RedrawMode, ViewerState},
};

pub(super) const ZOOM_STEP: f32 = 1.2;
/// パン1回で動かす量（表示範囲に対する割合）
const PAN_STEP: f32 = 0.25;

/// 配置のみの再描画を要求する。画像を描く再描画が既に予定されていればそれに任せる。
fn request_place(redraw_mode: &mut RedrawMode) {
    if matches!(*redraw_mode, RedrawMode::Idle | RedrawMode::HeaderRefresh) {
        *redraw_mode = RedrawMode::ImagePlace;
    }
}

/// ズーム倍率に必要な転送画像の解像度（画面に合わせた大きさの何倍か）。1, 2, 4, 8 の段階に丸める
pub(in crate::tui) fn render_scale(zoom: f32) -> u32 {
    if zoom <= 1.0 {
        1
    } else {
        (zoom.ceil() as u32)
            .next_power_of_two()
            .min(MAX_ZOOM as u32)
    }
}

/// 倍率の段階が変わったら、ズームが落ち着いた後（デバウンス後）にその解像度で準備し直す
fn sync_render_scale(state: &mut ViewerState, debounce: Duration) {
    let scale = render_scale(state.image_config.zoom_factor);
    if scale != state.image_config.render_scale {
        state.image_config.render_scale = scale;
        state.cache.invalidate_prepared();
        // 準備中の古い解像度の結果は捨てる
        state.preview.expected_preview_generation = None;
        state.pending_deadline = Some(Instant::now() + debounce);
    }
}

/// `factor` 倍に拡大縮小する。`anchor` (列, 行) を渡すとその位置の画素を動かさずに拡大する。
pub(super) fn zoom_by(
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    factor: f32,
    anchor: Option<(u16, u16)>,
    debounce: Duration,
) {
    let old = state.image_config.zoom_factor;
    let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
    if let Some(placement) = state.perf.last_image_rect {
        let (cx, cy) = placement.center();
        let (ux, uy) = anchor
            .and_then(|(col, row)| placement.image_point(col, row))
            .unwrap_or((cx, cy));
        // 画面上の anchor 位置を固定する: (u - c) * zoom が不変になるよう中心を動かす
        let k = old / new;
        state.image_config.pan_center = (ux - (ux - cx) * k, uy - (uy - cy) * k);
    }
    state.image_config.zoom_factor = new;
    request_place(redraw_mode);
    sync_render_scale(state, debounce);
}

pub(super) fn fit_image(redraw_mode: &mut RedrawMode, state: &mut ViewerState, debounce: Duration) {
    state.image_config.zoom_factor = 1.0;
    state.image_config.pan_center = (0.5, 0.5);
    request_place(redraw_mode);
    sync_render_scale(state, debounce);
}

/// キー操作のパン: 表示範囲の PAN_STEP 割合ずつ動かす（dx, dy は -1/0/1）
pub(super) fn pan_step(redraw_mode: &mut RedrawMode, state: &mut ViewerState, dx: f32, dy: f32) {
    pan_image(redraw_mode, state, dx * PAN_STEP, dy * PAN_STEP);
}

/// ドラッグのパン: つかんだ画素がカーソルに付いてくるよう、移動セル数の逆向きに動かす
pub(super) fn pan_drag(
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    dcols: i32,
    drows: i32,
) {
    let Some(placement) = state.perf.last_image_rect else {
        return;
    };
    let fx = -(dcols as f32) / placement.cols.max(1) as f32;
    let fy = -(drows as f32) / placement.rows.max(1) as f32;
    pan_image(redraw_mode, state, fx, fy);
}

/// 表示中心を表示範囲に対する割合 (fx, fy) だけ動かす
fn pan_image(redraw_mode: &mut RedrawMode, state: &mut ViewerState, fx: f32, fy: f32) {
    let Some(placement) = state.perf.last_image_rect else {
        return;
    };
    // 実際の切り出し中心から動かすので、端で押し続けても中心が画像外へ溜まらない
    let (cx, cy) = placement.center();
    let (vx, vy) = placement.visible();
    state.image_config.pan_center = (
        (cx + fx * vx).clamp(0.0, 1.0),
        (cy + fy * vy).clamp(0.0, 1.0),
    );
    request_place(redraw_mode);
}

#[cfg(test)]
mod tests {
    use super::render_scale;

    #[test]
    fn render_scale_steps_by_power_of_two() {
        let steps: Vec<u32> = [0.5, 1.0, 1.2, 2.0, 2.1, 4.0, 5.0, 8.0]
            .into_iter()
            .map(render_scale)
            .collect();
        assert_eq!(steps, vec![1, 1, 2, 2, 4, 4, 8, 8]);
    }
}
