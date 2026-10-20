use crossterm::event::{Event, KeyEventKind};

use super::super::input::{KeyProcessContext, process_key, process_mouse};
use super::super::state::RedrawMode;
use super::Viewer;

impl Viewer<'_> {
    /// イベントを処理し、(終了するか, 表示画像が変わったか) を返す
    pub(super) fn handle_event(&mut self, event: Event) -> (bool, bool) {
        let previous_index = self.current_index;

        let should_quit = match event {
            Event::Key(key) if key.kind == KeyEventKind::Press => process_key(
                key,
                KeyProcessContext {
                    image_files: &mut self.image_files,
                    current_index: &mut self.current_index,
                    redraw_mode: &mut self.redraw_mode,
                    state: &mut self.state,
                    debounce_duration: self.debounce_duration,
                    term_height: self.viewport.height,
                    sort_field: &mut self.sort_field,
                    sort_descending: &mut self.sort_descending,
                },
            ),
            Event::Mouse(mouse) => {
                process_mouse(
                    mouse,
                    &mut self.current_index,
                    &mut self.redraw_mode,
                    &mut self.state,
                    self.debounce_duration,
                    self.viewport.height,
                );
                false
            }
            Event::Resize(width, height) => {
                self.viewport.width = width;
                self.viewport.height = height;
                self.state.cache.invalidate_prepared();
                self.redraw_mode = RedrawMode::LayoutRefresh;
                false
            }
            _ => false,
        };

        (should_quit, self.current_index != previous_index)
    }
}
