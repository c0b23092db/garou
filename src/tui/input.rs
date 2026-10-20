use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use std::{path::PathBuf, time::Duration};

use crate::core::SortField;
use crate::model::keymap::Action;

mod clear;
mod file_ops;
mod input_move;
mod open;
mod sort;
mod zoom;

use super::render::filetree::SidebarTree;
use super::state::{NavDirection, RedrawMode, ViewerState};

use clear::{clear_and_full_refresh, clear_and_image_refresh};
use file_ops::{FileOpContext, handle_prompt_key, start_delete, start_rename};
use input_move::{apply_sidebar_cursor_change, schedule_redraw, sync_sidebar_to_image};
use open::open_current_image;
use sort::{SortContext, apply_sort};
use zoom::{ZOOM_STEP, fit_image, pan_drag, pan_step, zoom_by};

fn is_mouse_on_image(mouse: MouseEvent, state: &ViewerState) -> bool {
    state
        .perf
        .last_image_rect
        .is_some_and(|p| p.image_point(mouse.column, mouse.row).is_some())
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
    if state.prompt.is_some() {
        return handle_prompt_key(
            key,
            FileOpContext {
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field: *sort_field,
                sort_descending: *sort_descending,
            },
        );
    }
    let Some(action) = state.keymap.action(key) else {
        return false;
    };

    let sort = match action {
        Action::SortModified => Some((SortField::ModifiedTime, false)),
        Action::SortModifiedDesc => Some((SortField::ModifiedTime, true)),
        Action::SortNatural => Some((SortField::Natural, false)),
        Action::SortNaturalDesc => Some((SortField::Natural, true)),
        Action::SortSize => Some((SortField::Size, false)),
        Action::SortSizeDesc => Some((SortField::Size, true)),
        _ => None,
    };
    if let Some((field, descending)) = sort {
        apply_sort(
            SortContext {
                image_files,
                current_index,
                redraw_mode,
                state,
                sort_field,
                sort_descending,
            },
            field,
            descending,
        );
        return false;
    }

    let page_rows = usize::from(term_height.saturating_sub(2)).max(1);
    let last_index = image_count - 1;
    let sidebar_visible = state.ui_state.sidebar_visible;
    let mut nav = NavContext {
        image_files,
        current_index,
        redraw_mode,
        state,
        debounce_duration,
    };
    match action {
        Action::Quit => return true,
        Action::Open => open_current_image(nav.image_files, *nav.current_index),
        Action::Delete => start_delete(nav.redraw_mode, nav.state),
        Action::Rename => start_rename(
            nav.image_files,
            *nav.current_index,
            nav.redraw_mode,
            nav.state,
        ),
        Action::ToggleSidebar => {
            nav.state.ui_state.sidebar_visible = !sidebar_visible;
            clear_and_full_refresh(nav.redraw_mode, nav.state);
        }
        Action::ToggleStatusbar => {
            nav.state.ui_state.statusbar_visible = !nav.state.ui_state.statusbar_visible;
            clear_and_full_refresh(nav.redraw_mode, nav.state);
        }
        Action::ToggleHeader => {
            nav.state.ui_state.header_visible = !nav.state.ui_state.header_visible;
            clear_and_full_refresh(nav.redraw_mode, nav.state);
        }
        Action::ToggleOverlay => {
            nav.state.ui_state.overlay_visible = !nav.state.ui_state.overlay_visible;
            clear_and_full_refresh(nav.redraw_mode, nav.state);
        }
        Action::RefreshImage => clear_and_image_refresh(nav.redraw_mode, nav.state),
        Action::RefreshScreen => clear_and_full_refresh(nav.redraw_mode, nav.state),
        // サイドバー非表示時の上下移動は何もしない
        Action::Up | Action::Down if sidebar_visible => {
            let delta = if action == Action::Down { 1 } else { -1 };
            if nav.state.sidebar_tree.move_cursor(delta) {
                nav.apply_sidebar_cursor_change();
            }
        }
        Action::PageUp => {
            let target = nav.current_index.saturating_sub(page_rows);
            nav.jump(
                |t| t.move_cursor_page(-1, page_rows),
                target,
                NavDirection::Backward,
            );
        }
        Action::PageDown => {
            let target = (*nav.current_index + page_rows).min(last_index);
            nav.jump(
                |t| t.move_cursor_page(1, page_rows),
                target,
                NavDirection::Forward,
            );
        }
        Action::First => nav.jump(|t| t.move_to_start(), 0, NavDirection::Backward),
        Action::Last => nav.jump(|t| t.move_to_end(), last_index, NavDirection::Forward),
        // ディレクトリの展開/折りたたみ（サイドバー表示時）
        Action::ToggleDir if sidebar_visible => {
            if nav.state.sidebar_tree.toggle_current_dir() {
                *nav.redraw_mode = RedrawMode::HeaderRefresh;
            } else {
                nav.apply_sidebar_cursor_change();
            }
        }
        // サイドバー表示時はディレクトリの折りたたみ/展開、非表示時は画像の前後移動
        Action::Prev if sidebar_visible => {
            if nav.state.sidebar_tree.collapse_current_dir() {
                *nav.redraw_mode = RedrawMode::HeaderRefresh;
            }
        }
        Action::Next if sidebar_visible => {
            if nav.state.sidebar_tree.expand_current_dir() {
                *nav.redraw_mode = RedrawMode::HeaderRefresh;
            }
        }
        Action::Prev => {
            let target = nav.current_index.checked_sub(1).unwrap_or(last_index);
            nav.step_image(target, NavDirection::Backward);
        }
        Action::Next => {
            let target = if *nav.current_index < last_index {
                *nav.current_index + 1
            } else {
                0
            };
            nav.step_image(target, NavDirection::Forward);
        }
        Action::PanLeft => pan_step(nav.redraw_mode, nav.state, -1.0, 0.0),
        Action::PanRight => pan_step(nav.redraw_mode, nav.state, 1.0, 0.0),
        Action::PanUp => pan_step(nav.redraw_mode, nav.state, 0.0, -1.0),
        Action::PanDown => pan_step(nav.redraw_mode, nav.state, 0.0, 1.0),
        Action::ZoomIn => zoom_by(
            nav.redraw_mode,
            nav.state,
            ZOOM_STEP,
            None,
            debounce_duration,
        ),
        Action::ZoomOut => zoom_by(
            nav.redraw_mode,
            nav.state,
            1.0 / ZOOM_STEP,
            None,
            debounce_duration,
        ),
        Action::ZoomFit => fit_image(nav.redraw_mode, nav.state, debounce_duration),
        _ => {}
    }
    false
}

/// 画像・サイドバー移動に必要な参照のまとまり
struct NavContext<'a> {
    image_files: &'a [PathBuf],
    current_index: &'a mut usize,
    redraw_mode: &'a mut RedrawMode,
    state: &'a mut ViewerState,
    debounce_duration: Duration,
}

impl NavContext<'_> {
    fn apply_sidebar_cursor_change(&mut self) {
        apply_sidebar_cursor_change(
            self.current_index,
            self.redraw_mode,
            self.state,
            self.debounce_duration,
        );
    }

    /// 表示画像を target に切り替え、サイドバーを追従させる
    fn step_image(&mut self, target: usize, direction: NavDirection) {
        *self.current_index = target;
        sync_sidebar_to_image(target, self.state, self.image_files, direction);
        schedule_redraw(self.redraw_mode, self.state, self.debounce_duration);
    }

    /// サイドバー表示時はカーソルを `move_sidebar` で、非表示時は画像を target へ移動する
    fn jump(
        &mut self,
        move_sidebar: impl FnOnce(&mut SidebarTree) -> bool,
        target: usize,
        direction: NavDirection,
    ) {
        if self.state.ui_state.sidebar_visible {
            if move_sidebar(&mut self.state.sidebar_tree) {
                self.apply_sidebar_cursor_change();
            }
        } else if *self.current_index != target {
            self.step_image(target, direction);
        }
    }
}

/// マウス入力を処理し、必要に応じて描画モードを更新する
pub fn process_mouse(
    mouse: MouseEvent,
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
    term_height: u16,
) {
    match mouse.kind {
        // 画像上のホイール: カーソル位置を中心に拡大縮小
        MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
            if is_mouse_on_image(mouse, state) =>
        {
            let factor = if mouse.kind == MouseEventKind::ScrollUp {
                ZOOM_STEP
            } else {
                1.0 / ZOOM_STEP
            };
            zoom_by(
                redraw_mode,
                state,
                factor,
                Some((mouse.column, mouse.row)),
                debounce_duration,
            );
            return;
        }
        // 画像上で左ボタンを押したらドラッグでパンする
        MouseEventKind::Down(MouseButton::Left) if is_mouse_on_image(mouse, state) => {
            state.drag_origin = Some((mouse.column, mouse.row));
            return;
        }
        MouseEventKind::Drag(MouseButton::Left) => {
            if let Some((col, row)) = state.drag_origin {
                let dcols = i32::from(mouse.column) - i32::from(col);
                let drows = i32::from(mouse.row) - i32::from(row);
                if dcols != 0 || drows != 0 {
                    pan_drag(redraw_mode, state, dcols, drows);
                    state.drag_origin = Some((mouse.column, mouse.row));
                }
            }
            return;
        }
        MouseEventKind::Up(MouseButton::Left) if state.drag_origin.is_some() => {
            state.drag_origin = None;
            return;
        }
        _ => {}
    }

    if !state.ui_state.sidebar_visible || mouse.column >= state.ui_state.sidebar_size.max(1) {
        return;
    }

    match mouse.kind {
        MouseEventKind::ScrollUp => {
            if state.sidebar_tree.move_cursor(-1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            return;
        }
        MouseEventKind::ScrollDown => {
            if state.sidebar_tree.move_cursor(1) {
                apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
            }
            return;
        }
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return,
    }

    if !state
        .sidebar_tree
        .set_cursor_by_screen_row(mouse.row, term_height)
    {
        return;
    }

    if state.sidebar_tree.toggle_current_dir() {
        *redraw_mode = RedrawMode::HeaderRefresh;
        return;
    }

    apply_sidebar_cursor_change(current_index, redraw_mode, state, debounce_duration);
}
