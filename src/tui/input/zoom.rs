use super::{
    super::debounce::clear_pending_replace,
    super::state::{RedrawMode, ViewerState},
};

const ZOOM_STEP: f32 = 1.2;
const FIT_ZOOM: f32 = 1.0;

pub(super) fn zoom_in(redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    apply_zoom(state.zoom_factor() * ZOOM_STEP, redraw_mode, state);
}

pub(super) fn zoom_out(redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    apply_zoom(state.zoom_factor() / ZOOM_STEP, redraw_mode, state);
}

pub(super) fn fit_image(redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    state.reset_pan();
    apply_zoom(FIT_ZOOM, redraw_mode, state);
}

fn apply_zoom(new_zoom: f32, redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    state.set_zoom_factor(new_zoom);
    clear_pending_replace(state);
    *redraw_mode = RedrawMode::ImageRefresh;
}
