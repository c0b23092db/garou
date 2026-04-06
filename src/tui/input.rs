use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use std::{path::PathBuf, time::Duration};

use crate::core::SortField;

mod clear;
mod input_move;
mod open;
mod pan;
mod sort;
mod zoom;

use super::state::{NavDirection, RedrawMode, ViewerState};

use clear::{clear_and_full_refresh, clear_and_image_refresh};
use input_move::{apply_sidebar_cursor_change, schedule_redraw, sync_sidebar_to_image};
use open::open_current_image;
use pan::pan_image;
use sort::{SortContext, apply_sort};
use zoom::{fit_image, zoom_in, zoom_out};

fn is_mouse_on_image(mouse: MouseEvent, state: &ViewerState) -> bool {
    let Some((x, y, w, h)) = state.last_image_rect() else {
        return false;
    };
    let x_end = u32::from(x).saturating_add(w);
    let y_end = u32::from(y).saturating_add(h);
    let mx = u32::from(mouse.column);
    let my = u32::from(mouse.row);
    mx >= u32::from(x) && mx < x_end && my >= u32::from(y) && my < y_end
}

/// キー入力を処理し、必要に応じて描画モードを更新する
pub struct KeyProcessContext<'a> {
    pub image_files: &'a mut Vec<PathBuf>,
    pub current_index: &'a mut usize,
    pub redraw_mode: &'a mut RedrawMode,
    pub state: &'a mut ViewerState,
    pub debounce_duration: Duration,
    pub term_height: u16,
    pub sort_field: &'a mut SortField,
    pub sort_descending: &'a mut bool,
}

pub fn process_key(key: KeyEvent, ctx: KeyProcessContext<'_>) -> bool {
    let KeyProcessContext {
        image_files,
        current_index,
        redraw_mode,
        state,
        debounce_duration,
        term_height,
        sort_field,
        sort_descending,
    } = ctx;

    let image_count = image_files.len();
    if image_count == 0 {
        return false;
    }

    let page_rows = usize::from(term_height.saturating_sub(2)).max(1);
    match key.code {
        // q: 終了
        KeyCode::Char('q') | KeyCode::Esc => true,
        // o: 既定の画像ビューアで開く
        KeyCode::Char('o') | KeyCode::Char('O') => {
            open_current_image(image_files, *current_index);
            false
        }
        // Alt+s: サイドバーの表示切替
        KeyCode::Char('s') | KeyCode::Char('S') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_sidebar_visible(!state.sidebar_visible());
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        // Alt+d: ステータスバーの表示切替
        KeyCode::Char('d') | KeyCode::Char('D') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_statusbar_visible(!state.statusbar_visible());
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        // Alt+f: ヘッダーの表示切替
        KeyCode::Char('f') | KeyCode::Char('F') if key.modifiers.contains(KeyModifiers::ALT) => {
            state.set_header_visible(!state.header_visible());
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        // r: 画像の再読み込み
        KeyCode::Char('r') => {
            clear_and_image_refresh(redraw_mode, state);
            false
        }
        // R: 画面の完全な再描画
        KeyCode::Char('R') => {
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        // Tab: 画像情報オーバーレイの表示切替
        KeyCode::Tab => {
            state.set_overlay_visible(!state.overlay_visible());
            clear_and_full_refresh(redraw_mode, state);
            false
        }
        /* サイドバーのカーソル移動 */
        // j/k: サイドバーでのカーソル移動
        KeyCode::Char('j') | KeyCode::Down if state.sidebar_visible() => {
            if state.sidebar_tree.move_cursor(1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            false
        }
        KeyCode::Char('k') | KeyCode::Up if state.sidebar_visible() => {
            if state.sidebar_tree.move_cursor(-1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            false
        }
        // Ctrl+b: 一ページ前へ移動
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
        // Ctrl+f: 一ページ次へ移動
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
        // g: 先頭へ移動
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
        // G: 末尾へ移動
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
        // Enter: ディレクトリの展開/折りたたみ（サイドバー表示時）
        KeyCode::Enter if state.sidebar_visible() => {
            if state.sidebar_tree.toggle_current_dir() {
                *redraw_mode = RedrawMode::HeaderRefresh;
                return false;
            }
            apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            false
        }
        // h/l: 画像の前後移動（サイドバー非表示時）またはディレクトリの展開/折りたたみ（サイドバー表示時）
        KeyCode::Char('h') | KeyCode::Left => {
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
        KeyCode::Char('l') | KeyCode::Right => {
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
        /* 画像のズーム・パン */
        KeyCode::Char('H') => {
            pan_image(redraw_mode, state, -2, 0);
            false
        }
        KeyCode::Char('L') => {
            pan_image(redraw_mode, state, 2, 0);
            false
        }
        KeyCode::Char('J') => {
            pan_image(redraw_mode, state, 0, 1);
            false
        }
        KeyCode::Char('K') => {
            pan_image(redraw_mode, state, 0, -1);
            false
        }
        // +: ズームイン
        KeyCode::Char('+') => {
            zoom_in(redraw_mode, state);
            false
        }
        // -: ズームアウト
        KeyCode::Char('-') => {
            zoom_out(redraw_mode, state);
            false
        }
        // 0: 画像フィット
        KeyCode::Char('0') => {
            fit_image(redraw_mode, state);
            false
        }
        /* ソート */
        // m: 修正日時
        KeyCode::Char('m') => {
            apply_sort(
                SortContext {
                    image_files,
                    current_index,
                    redraw_mode,
                    state,
                    sort_field,
                    sort_descending,
                },
                SortField::ModifiedTime,
                false,
            );
            false
        }
        // M: 修正日時（降順）
        KeyCode::Char('M') => {
            apply_sort(
                SortContext {
                    image_files,
                    current_index,
                    redraw_mode,
                    state,
                    sort_field,
                    sort_descending,
                },
                SortField::ModifiedTime,
                true,
            );
            false
        }
        // n: 自然順
        KeyCode::Char('n') => {
            apply_sort(
                SortContext {
                    image_files,
                    current_index,
                    redraw_mode,
                    state,
                    sort_field,
                    sort_descending,
                },
                SortField::Natural,
                false,
            );
            false
        }
        // N: 自然順（降順）
        KeyCode::Char('N') => {
            apply_sort(
                SortContext {
                    image_files,
                    current_index,
                    redraw_mode,
                    state,
                    sort_field,
                    sort_descending,
                },
                SortField::Natural,
                true,
            );
            false
        }
        // s: サイズ
        KeyCode::Char('s') => {
            apply_sort(
                SortContext {
                    image_files,
                    current_index,
                    redraw_mode,
                    state,
                    sort_field,
                    sort_descending,
                },
                SortField::Size,
                false,
            );
            false
        }
        // S: サイズ（降順）
        KeyCode::Char('S') => {
            apply_sort(
                SortContext {
                    image_files,
                    current_index,
                    redraw_mode,
                    state,
                    sort_field,
                    sort_descending,
                },
                SortField::Size,
                true,
            );
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
    match mouse.kind {
        MouseEventKind::ScrollUp if is_mouse_on_image(mouse, state) => {
            zoom_in(redraw_mode, state);
            return false;
        }
        MouseEventKind::ScrollDown if is_mouse_on_image(mouse, state) => {
            zoom_out(redraw_mode, state);
            return false;
        }
        _ => {}
    }

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
