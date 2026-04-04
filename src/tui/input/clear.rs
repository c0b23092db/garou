use super::{
    super::debounce::clear_pending_replace,
    super::state::{RedrawMode, ViewerState},
};

pub(super) fn clear_and_full_refresh(redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    clear_pending_replace(state);
    *redraw_mode = RedrawMode::FullRefresh;
}

pub(super) fn clear_and_image_refresh(redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    clear_pending_replace(state);
    *redraw_mode = RedrawMode::ImageRefresh;
}
