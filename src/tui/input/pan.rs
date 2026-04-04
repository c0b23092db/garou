use super::{
    super::debounce::clear_pending_replace,
    super::state::{RedrawMode, ViewerState},
};

pub(super) fn pan_image(
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    dx: i16,
    dy: i16,
) {
    if let Some((x, y, _, _)) = state.last_image_rect() {
        let min_x = if state.sidebar_visible() {
            state.sidebar_size()
        } else {
            0
        };

        if dx < 0 && x <= min_x {
            return;
        }
        if dy < 0 && y <= 1 {
            return;
        }
    }

    state.pan_by(dx, dy);
    clear_pending_replace(state);
    *redraw_mode = RedrawMode::ImageRefresh;
}
