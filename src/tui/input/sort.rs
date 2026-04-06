use std::path::PathBuf;

use crate::core::{SortField, sort_image_files};

use super::{
    super::debounce::clear_pending_replace,
    super::state::{RedrawMode, ViewerState},
};

pub(super) struct SortContext<'a> {
    pub image_files: &'a mut Vec<PathBuf>,
    pub current_index: &'a mut usize,
    pub redraw_mode: &'a mut RedrawMode,
    pub state: &'a mut ViewerState,
    pub sort_field: &'a mut SortField,
    pub sort_descending: &'a mut bool,
}

pub(super) fn apply_sort(ctx: SortContext<'_>, new_field: SortField, new_descending: bool) {
    let SortContext {
        image_files,
        current_index,
        redraw_mode,
        state,
        sort_field,
        sort_descending,
    } = ctx;

    if image_files.is_empty() {
        return;
    }

    let current_path = image_files.get(*current_index).cloned();
    *sort_field = new_field;
    *sort_descending = new_descending;
    sort_image_files(image_files.as_mut_slice(), *sort_field, *sort_descending);

    if let Some(path) = &current_path
        && let Some(new_index) = image_files.iter().position(|p| p == path)
    {
        *current_index = new_index;
    }

    state
        .sidebar_tree
        .refresh_image_index_map(image_files, current_path.as_deref());

    // indexベースのキャッシュは並べ替え後に不整合となるため全クリアする。
    state.image_cache_mut().clear();
    state.image_dimensions_cache_mut().clear();
    state.payload_hash_cache_mut().clear();

    clear_pending_replace(state);
    *redraw_mode = RedrawMode::FullRefresh;
}
