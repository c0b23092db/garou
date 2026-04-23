use anyhow::{Result, anyhow};
use std::cmp::Ordering;
use std::env::current_dir;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortField {
    Natural,
    ModifiedTime,
    Size,
}

/// 画像ファイルのリストを取得し、開始位置を決定する関数
pub fn resolve_image_start(
    path: Option<PathBuf>,
    extensions: &[String],
) -> Result<(Vec<PathBuf>, usize)> {
    match path {
        None => {
            // カレントディレクトリ //
            let dir = current_dir().unwrap_or_else(|_| PathBuf::from("."));
            if !has_image_file_in_dir(&dir, extensions)? {
                return Err(anyhow!(
                    "カレントディレクトリ直下に画像ファイルがありません: {}",
                    dir.display()
                ));
            }
            let files = get_image_files_from_dir(&dir, extensions)?;
            Ok((files, 0))
        }
        Some(p) => {
            if p.is_dir() {
                // ディレクトリ指定 //
                if !has_image_file_in_dir(&p, extensions)? {
                    return Err(anyhow!(
                        "指定ディレクトリ直下に画像ファイルがありません: {}",
                        p.display()
                    ));
                }
                let files = get_image_files_from_dir(&p, extensions)?;
                Ok((files, 0))
            } else {
                // ファイル指定 //
                let parent = p
                    .parent()
                    .ok_or_else(|| anyhow!("No parent directory for path: {:?}", p))?;
                let files = get_image_files_from_dir(parent, extensions)?;
                let start_index = files.iter().position(|f| f == &p).unwrap_or(0);
                Ok((files, start_index))
            }
        }
    }
}

/// 指定されたディレクトリから画像ファイルを再帰的に収集する関数
fn get_image_files_from_dir(dir: &Path, extensions: &[String]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_image_files_recursive(dir, &mut files, extensions)?;
    Ok(files)
}

/// 指定されたディレクトリに画像ファイルが存在するかをチェックする関数
/// ## Returns
/// - `Ok(true)` 画像ファイルが存在する場合
/// - `Ok(false)` 画像ファイルが存在しない場合
/// - `Err` ディレクトリの読み込みに失敗した場合など
fn has_image_file_in_dir(dir: &Path, extensions: &[String]) -> Result<bool> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_file() && is_image_path(&path, extensions) {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 指定されたディレクトリを再帰的に走査して画像ファイルを収集する関数
fn collect_image_files_recursive(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    extensions: &[String],
) -> Result<()> {
    let mut dirs = Vec::new();
    let mut local_files = Vec::new();

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if is_image_path(&path, extensions) {
            local_files.push(path);
        }
    }

    dirs.sort_by(|a, b| natural_compare_paths_by_name(a, b));
    local_files.sort_by(|a, b| natural_compare_paths_by_name(a, b));

    files.extend(local_files);
    for subdir in dirs {
        collect_image_files_recursive(&subdir, files, extensions)?;
    }

    Ok(())
}

/// 指定されたパスが画像ファイルであるかを判定する関数
pub fn is_image_path(path: &Path, extensions: &[String]) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            let ext = ext.to_ascii_lowercase();
            extensions.iter().any(|allowed| {
                let allowed = allowed.trim().trim_start_matches('.').to_ascii_lowercase();
                !allowed.is_empty() && ext == allowed
            })
        })
        .unwrap_or(false)
}

/// ファイル名を自然順で比較する関数
pub fn natural_compare_paths_by_name(a: &Path, b: &Path) -> Ordering {
    let a_name = a.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    let b_name = b.file_name().and_then(|n| n.to_str()).unwrap_or_default();
    natural_compare(a_name, b_name)
}

/// 画像ファイルリストを指定条件でソートする関数
pub fn sort_image_files(image_files: &mut [PathBuf], sort_field: SortField, descending: bool) {
    image_files.sort_by(|a, b| {
        let ord = match sort_field {
            SortField::Natural => natural_compare_paths_by_name(a, b),
            SortField::ModifiedTime => compare_modified_time(a, b),
            SortField::Size => compare_file_size(a, b),
        }
        .then_with(|| natural_compare_paths_by_name(a, b));

        if descending { ord.reverse() } else { ord }
    });
}

/// ファイルの最終更新日時を比較する関数
fn compare_modified_time(a: &Path, b: &Path) -> Ordering {
    let a_time = fs::metadata(a)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let b_time = fs::metadata(b)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis())
        .unwrap_or(0);
    a_time.cmp(&b_time)
}

/// ファイルサイズを比較する関数
fn compare_file_size(a: &Path, b: &Path) -> Ordering {
    let a_size = fs::metadata(a).map(|m| m.len()).unwrap_or(0);
    let b_size = fs::metadata(b).map(|m| m.len()).unwrap_or(0);
    a_size.cmp(&b_size)
}

/// 文字列を自然順で比較する関数
pub fn natural_compare(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek(), b_chars.peek()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(&a_ch), Some(&b_ch)) => {
                if a_ch.is_ascii_digit() && b_ch.is_ascii_digit() {
                    let mut a_num = String::new();
                    while let Some(&ch) = a_chars.peek() {
                        if ch.is_ascii_digit() {
                            a_num.push(ch);
                            a_chars.next();
                        } else {
                            break;
                        }
                    }

                    let mut b_num = String::new();
                    while let Some(&ch) = b_chars.peek() {
                        if ch.is_ascii_digit() {
                            b_num.push(ch);
                            b_chars.next();
                        } else {
                            break;
                        }
                    }

                    let a_val: u64 = a_num.parse().unwrap_or(0);
                    let b_val: u64 = b_num.parse().unwrap_or(0);
                    match a_val.cmp(&b_val) {
                        Ordering::Equal => continue,
                        other => return other,
                    }
                } else if a_ch == b_ch {
                    a_chars.next();
                    b_chars.next();
                } else {
                    return a_ch.cmp(&b_ch);
                }
            }
        }
    }
}
