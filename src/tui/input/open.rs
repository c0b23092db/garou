use std::path::PathBuf;

pub(super) fn open_current_image(image_files: &[PathBuf], current_index: usize) {
    if let Some(image_path) = image_files.get(current_index) {
        let _ = open::that(image_path);
    }
}
