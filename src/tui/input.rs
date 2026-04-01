use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::{path::PathBuf, time::Duration};

use super::{
    debounce::{clear_pending_replace, schedule_replace},
    state::{NavDirection, RedrawMode, ViewerState},
};

/// デバウンス設定に基づいて描画モードをスケジュール
#[inline]
fn schedule_redraw(
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
fn apply_sidebar_cursor_change(
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
fn sync_sidebar_to_image(
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

/// キー入力を処理し、必要に応じて描画モードを更新する
pub fn process_key(
    key: KeyEvent,
    image_files: &[PathBuf],
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
    term_height: u16,
) -> bool {
    let image_count = image_files.len();
    if image_count == 0 {
        return false;
    }

    let page_rows = usize::from(term_height.saturating_sub(2)).max(1);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('o') | KeyCode::Char('O') => {
            if let Some(image_path) = image_files.get(*current_index) {
                let _ = open::that(image_path);
            }
            false
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let moved = if state.sidebar_visible() {
                state.sidebar_tree.move_cursor_page(-1, page_rows)
            } else {
                let old = *current_index;
                *current_index = current_index.saturating_sub(page_rows);
                *current_index != old
            };

            if moved {
                if state.sidebar_visible() {
                    apply_sidebar_cursor_change(
                        current_index,
                        redraw_mode,
                        state,
                        debounce_duration,
                    );
                } else {
                    sync_sidebar_to_image(
                        *current_index,
                        state,
                        image_files,
                        NavDirection::Backward,
                    );
                    schedule_redraw(redraw_mode, state, debounce_duration);
                }
            }
            false
        }
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let moved = if state.sidebar_visible() {
                state.sidebar_tree.move_cursor_page(1, page_rows)
            } else {
                let old = *current_index;
                *current_index = (*current_index + page_rows).min(image_count.saturating_sub(1));
                *current_index != old
            };

            if moved {
                if state.sidebar_visible() {
                    apply_sidebar_cursor_change(
                        current_index,
                        redraw_mode,
                        state,
                        debounce_duration,
                    );
                } else {
                    sync_sidebar_to_image(
                        *current_index,
                        state,
                        image_files,
                        NavDirection::Forward,
                    );
                    schedule_redraw(redraw_mode, state, debounce_duration);
                }
            }
            false
        }
        KeyCode::Char('s') | KeyCode::Char('S') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_sidebar_visible(!state.sidebar_visible());
            clear_pending_replace(state);
            *redraw_mode = RedrawMode::FullRefresh;
            false
        }
        KeyCode::Char('d') | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_statusbar_visible(!state.statusbar_visible());
            clear_pending_replace(state);
            *redraw_mode = RedrawMode::FullRefresh;
            false
        }
        KeyCode::Char('f') | KeyCode::Char('F') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_header_visible(!state.header_visible());
            clear_pending_replace(state);
            *redraw_mode = RedrawMode::FullRefresh;
            false
        }
        KeyCode::Char('r') => {
            clear_pending_replace(state);
            *redraw_mode = RedrawMode::ImageRefresh;
            false
        }
        KeyCode::Char('R') => {
            clear_pending_replace(state);
            *redraw_mode = RedrawMode::FullRefresh;
            false
        }
        KeyCode::Char('j') | KeyCode::Char('J') | KeyCode::Down if state.sidebar_visible() => {
            if state.sidebar_tree.move_cursor(1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            false
        }
        KeyCode::Char('k') | KeyCode::Char('K') | KeyCode::Up if state.sidebar_visible() => {
            if state.sidebar_tree.move_cursor(-1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            false
        }
        KeyCode::Enter if state.sidebar_visible() => {
            if state.sidebar_tree.toggle_current_dir() {
                *redraw_mode = RedrawMode::HeaderRefresh;
                return false;
            }
            apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            false
        }
        KeyCode::Char('g') => {
            let moved = if state.sidebar_visible() {
                state.sidebar_tree.move_to_start()
            } else {
                let old = *current_index;
                *current_index = 0;
                *current_index != old
            };

            if moved {
                if state.sidebar_visible() {
                    apply_sidebar_cursor_change(
                        current_index,
                        redraw_mode,
                        state,
                        debounce_duration,
                    );
                } else {
                    sync_sidebar_to_image(
                        *current_index,
                        state,
                        image_files,
                        NavDirection::Backward,
                    );
                    schedule_redraw(redraw_mode, state, debounce_duration);
                }
            }
            false
        }
        KeyCode::Char('G') => {
            let moved = if state.sidebar_visible() {
                state.sidebar_tree.move_to_end()
            } else {
                let old = *current_index;
                *current_index = image_count.saturating_sub(1);
                *current_index != old
            };

            if moved {
                if state.sidebar_visible() {
                    apply_sidebar_cursor_change(
                        current_index,
                        redraw_mode,
                        state,
                        debounce_duration,
                    );
                } else {
                    sync_sidebar_to_image(
                        *current_index,
                        state,
                        image_files,
                        NavDirection::Forward,
                    );
                    schedule_redraw(redraw_mode, state, debounce_duration);
                }
            }
            false
        }
        KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Left => {
            if state.sidebar_visible() {
                if state.sidebar_tree.collapse_current_dir() {
                    *redraw_mode = RedrawMode::HeaderRefresh;
                }
                return false;
            }

            state.last_nav_direction = NavDirection::Backward;
            if *current_index == 0 {
                *current_index = image_count - 1;
            } else {
                *current_index -= 1;
            }
            sync_sidebar_to_image(*current_index, state, image_files, NavDirection::Backward);
            schedule_redraw(redraw_mode, state, debounce_duration);
            false
        }
        KeyCode::Char('l') | KeyCode::Char('L') | KeyCode::Right => {
            if state.sidebar_visible() {
                if state.sidebar_tree.expand_current_dir() {
                    *redraw_mode = RedrawMode::HeaderRefresh;
                }
                return false;
            }

            state.last_nav_direction = NavDirection::Forward;
            if *current_index + 1 < image_count {
                *current_index += 1;
            } else {
                *current_index = 0;
            }
            sync_sidebar_to_image(*current_index, state, image_files, NavDirection::Forward);
            schedule_redraw(redraw_mode, state, debounce_duration);
            false
        }
        _ => false,
    }
}

/// マウス入力を処理し、必要に応じて描画モードを更新する
pub fn process_mouse(
    mouse: MouseEvent,
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
    sidebar_width: u16,
    term_height: u16,
) -> bool {
    if !state.sidebar_visible() {
        return false;
    }

    if mouse.column >= sidebar_width {
        return false;
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if state.sidebar_tree.move_cursor(-1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            return false;
        }
        MouseEventKind::ScrollDown => {
            if state.sidebar_tree.move_cursor(1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            return false;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return false,
    }

    if !state
        .sidebar_tree
        .set_cursor_by_screen_row(mouse.row, term_height)
    {
        return false;
    }

    if state.sidebar_tree.toggle_current_dir() {
        *redraw_mode = RedrawMode::HeaderRefresh;
        return false;
    }

    apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
    false
}
