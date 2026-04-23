use super::{
    super::debounce::clear_pending_replace,
    super::state::{RedrawMode, ViewerState},
};

pub(super) fn pan_image(redraw_mode: &mut RedrawMode, state: &mut ViewerState, dx: i16, dy: i16) {
    if let Some((x, y, _, _)) = state.perf.last_image_rect {
        let base_x = if state.ui_state.sidebar_visible {
            i32::from(state.ui_state.sidebar_size)
        } else {
            0
        };

        let mut pan_x = i32::from(state.image_config.pan_x);
        let mut pan_y = i32::from(state.image_config.pan_y);

        // 左端に張り付いたときの負方向オーバーランを除去
        let min_pan_x = -base_x;
        if x == 0 && pan_x < min_pan_x {
            pan_x = min_pan_x;
        }

        // 右端(u16上限)に張り付いたときの正方向オーバーランを除去
        let max_pan_x = i32::from(u16::MAX).saturating_sub(base_x);
        if x == u16::MAX && pan_x > max_pan_x {
            pan_x = max_pan_x;
        }

        // 上端に張り付いたときの負方向オーバーランを除去
        if y == 0 && pan_y < -1 {
            pan_y = -1;
        }

        let pan_x = pan_x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let pan_y = pan_y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        if pan_x != state.image_config.pan_x || pan_y != state.image_config.pan_y {
            state.image_config.pan_x = pan_x;
            state.image_config.pan_y = pan_y;
        }
    }

    state.image_config.pan_x = state.image_config.pan_x.saturating_add(dx);
    state.image_config.pan_y = state.image_config.pan_y.saturating_add(dy);
    clear_pending_replace(state);
    *redraw_mode = RedrawMode::ImageRefresh;
}
