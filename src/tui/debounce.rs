use std::time::{Duration, Instant};

use super::state::ViewerState;

/// スケジュールされた画像置換を実行する
pub(super) fn schedule_replace(state: &mut ViewerState, debounce_duration: Duration) {
    if debounce_duration.is_zero() {
        state.pending_replace = false;
        state.pending_deadline = None;
    } else {
        state.pending_replace = true;
        state.pending_deadline = Some(Instant::now() + debounce_duration);
    }
}

/// 保留中の画像置換があれば実行する
pub(super) fn clear_pending_replace(state: &mut ViewerState) {
    state.pending_replace = false;
    state.pending_deadline = None;
}
