use std::{path::PathBuf, time::Duration};

use super::{
    super::debounce::schedule_replace,
    super::state::{NavDirection, RedrawMode, ViewerState},
};

/// デバウンス設定に基づいて描画モードをスケジュール
#[inline]
pub(super) fn schedule_redraw(
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
) {
    *redraw_mode = if debounce_duration.is_zero() {
        RedrawMode::ImageReplace
    } else {
        RedrawMode::HeaderRefresh
    };
    schedule_replace(state, debounce_duration);
}

/// サイドバーカーソル移動後の処理を統一
#[inline]
pub(super) fn apply_sidebar_cursor_change(
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
) {
    if let Some(new_index) = state.sidebar_tree.cursor_image_index() {
        *current_index = new_index;
        schedule_redraw(redraw_mode, state, debounce_duration);
    } else {
        *redraw_mode = RedrawMode::HeaderRefresh;
    }
}

/// ナビゲーション後の共通処理（サイドバー非表示時）
#[inline]
pub(super) fn sync_sidebar_to_image(
    current_index: usize,
    state: &mut ViewerState,
    image_files: &[PathBuf],
    direction: NavDirection,
) {
    if let Some(current_path) = image_files.get(current_index) {
        state.sidebar_tree.sync_cursor_to_image(current_path);
    }
    state.last_nav_direction = direction;
}
