use crate::core::SortField;
use anyhow::Result;
use crossterm::event::{Event, KeyEventKind};
use std::{path::PathBuf, time::Duration};

use super::super::input::{KeyProcessContext, process_key, process_mouse};
use super::super::state::{RedrawMode, ViewerState, Viewport};

pub(super) struct EventContext<'a> {
    pub(super) image_files: &'a mut Vec<PathBuf>,
    pub(super) sort_field: &'a mut SortField,
    pub(super) sort_descending: &'a mut bool,
    pub(super) current_index: &'a mut usize,
    pub(super) redraw_mode: &'a mut RedrawMode,
    pub(super) state: &'a mut ViewerState,
    pub(super) viewport: &'a mut Viewport,
    pub(super) debounce_duration: Duration,
}

pub(super) fn handle_event(event: Event, ctx: &mut EventContext<'_>) -> Result<(bool, bool)> {
    let previous_index = *ctx.current_index;

    let should_quit = match event {
        Event::Key(key) if key.kind == KeyEventKind::Press => process_key(
            key,
            KeyProcessContext {
                image_files: ctx.image_files,
                current_index: ctx.current_index,
                redraw_mode: ctx.redraw_mode,
                state: ctx.state,
                debounce_duration: ctx.debounce_duration,
                term_height: ctx.viewport.height,
                sort_field: ctx.sort_field,
                sort_descending: ctx.sort_descending,
            },
        ),
        Event::Mouse(mouse) => process_mouse(
            mouse,
            ctx.current_index,
            ctx.redraw_mode,
            ctx.state,
            ctx.debounce_duration,
            ctx.state.ui_state.sidebar_size.max(1),
            ctx.viewport.height,
        ),
        Event::Resize(width, height) => {
            ctx.viewport.width = width;
            ctx.viewport.height = height;
            *ctx.redraw_mode = RedrawMode::LayoutRefresh;
            false
        }
        _ => false,
    };

    Ok((should_quit, *ctx.current_index != previous_index))
}
