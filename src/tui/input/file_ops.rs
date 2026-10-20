//! 表示中の画像のごみ箱移動・リネーム

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use crate::core::{SortField, sort_image_files};

use super::super::{
    debounce::clear_pending_replace,
    state::{Prompt, RedrawMode, ViewerState},
};

pub(super) struct FileOpContext<'a> {
    pub image_files: &'a mut Vec<PathBuf>,
    pub current_index: &'a mut usize,
    pub redraw_mode: &'a mut RedrawMode,
    pub state: &'a mut ViewerState,
    pub sort_field: SortField,
    pub sort_descending: bool,
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// 画面下部の入力欄を開く
fn open_prompt(redraw_mode: &mut RedrawMode, state: &mut ViewerState, prompt: Prompt) {
    state.prompt = Some(prompt);
    if *redraw_mode == RedrawMode::Idle {
        *redraw_mode = RedrawMode::HeaderRefresh;
    }
}

pub(super) fn start_delete(redraw_mode: &mut RedrawMode, state: &mut ViewerState) {
    open_prompt(redraw_mode, state, Prompt::ConfirmDelete);
}

pub(super) fn start_rename(
    image_files: &[PathBuf],
    current_index: usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
) {
    if let Some(path) = image_files.get(current_index) {
        open_prompt(redraw_mode, state, Prompt::Rename(file_name(path)));
    }
}

/// 入力欄が開いているときのキー入力。画像がなくなったら true（終了）
pub(super) fn handle_prompt_key(key: KeyEvent, ctx: FileOpContext<'_>) -> bool {
    let Some(prompt) = ctx.state.prompt.take() else {
        return false;
    };
    let result = match prompt {
        Prompt::ConfirmDelete if matches!(key.code, KeyCode::Char('y' | 'Y')) => {
            delete_current(ctx.image_files, ctx.current_index)
        }
        Prompt::Rename(mut name) => match key.code {
            KeyCode::Enter => rename_current(
                ctx.image_files,
                ctx.current_index,
                &name,
                ctx.sort_field,
                ctx.sort_descending,
            ),
            KeyCode::Esc => Ok(false),
            code => {
                match code {
                    KeyCode::Backspace => {
                        name.pop();
                    }
                    KeyCode::Char(c)
                        if !key
                            .modifiers
                            .intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) =>
                    {
                        name.push(c)
                    }
                    _ => {}
                }
                open_prompt(ctx.redraw_mode, ctx.state, Prompt::Rename(name));
                return false;
            }
        },
        // 確認の y 以外・メッセージ表示中は、どのキーでも閉じる
        _ => Ok(false),
    };

    match result {
        // 一覧が変わった: ツリーを作り直して描き直す
        Ok(true) => {
            if ctx.image_files.is_empty() {
                return true;
            }
            let current = ctx.image_files.get(*ctx.current_index).cloned();
            ctx.state
                .sidebar_tree
                .rebuild(ctx.image_files, current.as_deref());
            ctx.state.preview.expected_preview_generation = None;
            clear_pending_replace(ctx.state);
            *ctx.redraw_mode = RedrawMode::FullRefresh;
        }
        // 取り消し: 入力欄で隠れたステータスバーを通常の描画で戻す
        Ok(false) => ctx.state.pending_deadline = Some(Instant::now()),
        Err(message) => open_prompt(ctx.redraw_mode, ctx.state, Prompt::Message(message)),
    }
    false
}

/// 表示中の画像をごみ箱へ移す。成功したら true
fn delete_current(
    image_files: &mut Vec<PathBuf>,
    current_index: &mut usize,
) -> Result<bool, String> {
    let path = &image_files[*current_index];
    trash::delete(path).map_err(|e| format!("ごみ箱へ移動できません: {e}"))?;
    image_files.remove(*current_index);
    *current_index = (*current_index).min(image_files.len().saturating_sub(1));
    Ok(true)
}

/// 表示中の画像を同じフォルダ内で `name` に変える。変えたら true
fn rename_current(
    image_files: &mut [PathBuf],
    current_index: &mut usize,
    name: &str,
    sort_field: SortField,
    sort_descending: bool,
) -> Result<bool, String> {
    let name = name.trim();
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return Err(format!("使えない名前です: {name}"));
    }
    let old = image_files[*current_index].clone();
    let new = old.with_file_name(name);
    if new == old {
        return Ok(false);
    }
    // ponytail: 存在確認と rename の間に別プロセスが同名を作ると上書きされうる（Windows の rename は失敗する）
    if new.exists() {
        return Err(format!("同じ名前のファイルがあります: {name}"));
    }
    fs::rename(&old, &new).map_err(|e| format!("名前を変更できません: {e}"))?;
    image_files[*current_index] = new.clone();
    sort_image_files(image_files, sort_field, sort_descending);
    *current_index = image_files.iter().position(|p| *p == new).unwrap_or(0);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_validates_and_keeps_list_sorted() {
        let dir = std::env::temp_dir().join(format!("garou_rename_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let paths: Vec<PathBuf> = ["a.png", "b.png", "c.png"]
            .iter()
            .map(|n| dir.join(n))
            .collect();
        for p in &paths {
            fs::write(p, b"x").unwrap();
        }
        let mut files = paths.clone();
        let mut index = 0;

        let rename = |files: &mut Vec<PathBuf>, index: &mut usize, name: &str| {
            rename_current(files, index, name, SortField::Natural, false)
        };
        assert!(rename(&mut files, &mut index, "b.png").is_err()); // 既存
        assert!(rename(&mut files, &mut index, "x/y.png").is_err()); // 区切り文字
        assert_eq!(rename(&mut files, &mut index, "a.png"), Ok(false)); // 同名

        // a → d にすると末尾へ並び替わり、表示中の位置も追従する
        assert_eq!(rename(&mut files, &mut index, "d.png"), Ok(true));
        assert_eq!(
            files,
            vec![dir.join("b.png"), dir.join("c.png"), dir.join("d.png")]
        );
        assert_eq!(index, 2);
        assert!(dir.join("d.png").exists() && !dir.join("a.png").exists());

        fs::remove_dir_all(&dir).unwrap();
    }
}
