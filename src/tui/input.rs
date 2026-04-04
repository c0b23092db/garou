use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::{path::PathBuf, time::Duration};

use crate::core::SortField;

mod clear;
mod input_move;
mod open;
mod sort;
mod zoom;

use super::state::{NavDirection, RedrawMode, ViewerState};

use clear::{clear_and_full_refresh, clear_and_image_refresh};
use input_move::{apply_sidebar_cursor_change, schedule_redraw, sync_sidebar_to_image};
use open::open_current_image;
use sort::apply_sort;
use zoom::{fit_image, zoom_in, zoom_out};

/// キー入力を処理し、必要に応じて描画モードを更新する
pub fn process_key(
    key: KeyEvent,
    image_files: &mut Vec<PathBuf>,
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
    term_height: u16,
    sort_field: &mut SortField,
    sort_descending: &mut bool,
) -> bool {
    let image_count = image_files.len();
    if image_count == 0 {
        return false;
    }

    let page_rows = usize::from(term_height.saturating_sub(2)).max(1);
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => true,
        KeyCode::Char('o') | KeyCode::Char('O') => {
            open_current_image(image_files, *current_index);
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
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        KeyCode::Char('d') | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_statusbar_visible(!state.statusbar_visible());
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        KeyCode::Char('f') | KeyCode::Char('F') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_header_visible(!state.header_visible());
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        KeyCode::Char('r') => {
            clear_and_image_refresh(redraw_mode, state);
            false
        }
        KeyCode::Char('R') => {
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        KeyCode::Char('+')  => {
            zoom_in(redraw_mode, state);
            false
        }
        KeyCode::Char('-') => {
            zoom_out(redraw_mode, state);
            false
        }
        KeyCode::Char('0') => {
            fit_image(redraw_mode, state);
            false
        }
        KeyCode::Char(',') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                sort_field.next(),
                false,
            );
            false
        }
        KeyCode::Char('m') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                SortField::ModifiedTime,
                false,
            );
            false
        }
        KeyCode::Char('M') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                SortField::ModifiedTime,
                true,
            );
            false
        }
        KeyCode::Char('n') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                SortField::Natural,
                false,
            );
            false
        }
        KeyCode::Char('N') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                SortField::Natural,
                true,
            );
            false
        }
        KeyCode::Char('s') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                SortField::Size,
                false,
            );
            false
        }
        KeyCode::Char('S') => {
            apply_sort(
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
                SortField::Size,
                true,
            );
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
