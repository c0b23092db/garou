use std::time::{Duration, Instant};

use super::state::ViewerState;

/// デバウンス後の画像切り替えを予約する（0 なら予約しない）
pub(super) fn schedule_replace(state: &mut ViewerState, debounce_duration: Duration) {
    state.pending_deadline =
        (!debounce_duration.is_zero()).then(|| Instant::now() + debounce_duration);
}

/// 予約中の画像切り替えを取り消す
pub(super) fn clear_pending_replace(state: &mut ViewerState) {
    state.pending_deadline = None;
}
