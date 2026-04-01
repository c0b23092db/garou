use crate::core::{is_image_path, natural_compare_paths_by_name};
use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::{Path, PathBuf},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// ファイルツリーを描画する関数
#[derive(Debug, Clone)]
pub(super) struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    /// ルートからの深さ
    pub depth: usize,
    /// 親ノードのID（nodes内のインデックス）
    pub parent: Option<usize>,
    /// 子ノードのID（nodes内のインデックス）
    pub children: Vec<usize>,
}

/// ファイルツリーの状態を管理し、描画用のエントリを生成する構造体
#[derive(Debug, Clone)]
pub(crate) struct SidebarTree {
    nodes: Vec<TreeNode>,
    roots: Vec<usize>,
    visible_nodes: Vec<usize>,
    expanded_dirs: HashSet<PathBuf>,
    path_to_node: HashMap<PathBuf, usize>,
    image_index_by_path: HashMap<PathBuf, usize>,
    cursor_visible_index: usize,
}

/// ファイルツリーの状態を管理し、描画用のエントリを生成する構造体
impl SidebarTree {
    pub(in crate::tui) fn from_image_files(
        image_files: &[PathBuf],
        current_index: usize,
        extensions: &[String],
    ) -> Self {
        let root_dir = image_files
            .get(current_index)
            .or_else(|| image_files.first())
            .and_then(|path| path.parent())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let image_index_by_path = image_files
            .iter()
            .enumerate()
            .map(|(idx, path)| (path.clone(), idx))
            .collect::<HashMap<_, _>>();

        let mut tree = Self {
            nodes: Vec::new(),
            roots: Vec::new(),
            visible_nodes: Vec::new(),
            expanded_dirs: HashSet::new(),
            path_to_node: HashMap::new(),
            image_index_by_path,
            cursor_visible_index: 0,
        };

        tree.roots = build_tree_nodes(
            &root_dir,
            None,
            0,
            &mut tree.nodes,
            &mut tree.path_to_node,
            extensions,
        );
        tree.rebuild_visible();

        if let Some(current_path) = image_files.get(current_index) {
            tree.reveal_path(current_path);
            tree.set_cursor_to_path(current_path);
        }

        tree
    }

    /// visible_nodesを再構築する関数
    fn rebuild_visible(&mut self) {
        self.visible_nodes.clear();
        let roots = self.roots.clone();
        for root in roots {
            self.push_visible(root);
        }
        if self.visible_nodes.is_empty() {
            self.cursor_visible_index = 0;
        } else {
            self.cursor_visible_index = self
                .cursor_visible_index
                .min(self.visible_nodes.len().saturating_sub(1));
        }
    }

    /// node_idをvisible_nodesに追加する関数
    fn push_visible(&mut self, node_id: usize) {
        self.visible_nodes.push(node_id);
        let node = &self.nodes[node_id];
        if node.is_dir && self.expanded_dirs.contains(&node.path) {
            let children = node.children.clone();
            for child in children {
                self.push_visible(child);
            }
        }
    }

    /// 現在のカーソル位置のnode_idを取得する関数
    fn current_node_id(&self) -> Option<usize> {
        self.visible_nodes.get(self.cursor_visible_index).copied()
    }

    /// カーソルを上下に移動する関数
    pub(in crate::tui) fn move_cursor(&mut self, delta: isize) -> bool {
        if self.visible_nodes.is_empty() {
            return false;
        }
        let old = self.cursor_visible_index;
        if delta < 0 {
            self.cursor_visible_index = self.cursor_visible_index.saturating_sub((-delta) as usize);
        } else {
            self.cursor_visible_index = (self.cursor_visible_index + delta as usize)
                .min(self.visible_nodes.len().saturating_sub(1));
        }
        self.cursor_visible_index != old
    }

    /// カーソルを先頭に移動する関数
    pub(in crate::tui) fn move_to_start(&mut self) -> bool {
        if self.visible_nodes.is_empty() {
            return false;
        }

        let old = self.cursor_visible_index;
        self.cursor_visible_index = 0;
        self.cursor_visible_index != old
    }

    /// カーソルを末尾に移動する関数
    pub(in crate::tui) fn move_to_end(&mut self) -> bool {
        if self.visible_nodes.is_empty() {
            return false;
        }

        let old = self.cursor_visible_index;
        self.cursor_visible_index = self.visible_nodes.len().saturating_sub(1);
        self.cursor_visible_index != old
    }

    /// ページ単位でカーソルを移動する関数
    pub(in crate::tui) fn move_cursor_page(&mut self, delta: isize, rows: usize) -> bool {
        if rows == 0 {
            return false;
        }

        let step = (rows as isize).saturating_mul(delta);
        self.move_cursor(step)
    }

    /// visible_nodesの中で、カーソルを中心にして表示すべき開始位置を計算する関数
    fn visible_start_for_rows(&self, rows: usize) -> usize {
        if self.visible_nodes.len() <= rows {
            0
        } else {
            let half = rows / 2;
            self.cursor_visible_index
                .saturating_sub(half)
                .min(self.visible_nodes.len().saturating_sub(rows))
        }
    }

    /// 画面の行数に応じてカーソル位置を更新する関数
    pub(in crate::tui) fn set_cursor_by_screen_row(&mut self, row: u16, term_height: u16) -> bool {
        if self.visible_nodes.is_empty() || row == 0 {
            return false;
        }

        let rows = usize::from(term_height.saturating_sub(2));
        if rows == 0 {
            return false;
        }

        let viewport_row = usize::from(row - 1);
        if viewport_row >= rows {
            return false;
        }

        let start = self.visible_start_for_rows(rows);
        let new_index = start + viewport_row;
        if new_index >= self.visible_nodes.len() {
            return false;
        }

        let old = self.cursor_visible_index;
        self.cursor_visible_index = new_index;
        self.cursor_visible_index != old
    }

    /// カーソル位置のファイルがディレクトリであれば展開/折りたたみを切り替える関数
    pub(in crate::tui) fn cursor_image_index(&self) -> Option<usize> {
        let node_id = self.current_node_id()?;
        let node = &self.nodes[node_id];
        if node.is_dir {
            None
        } else {
            self.image_index_by_path.get(&node.path).copied()
        }
    }

    /// カーソル位置のファイルがディレクトリであれば展開/折りたたみを切り替える関数
    pub(in crate::tui) fn toggle_current_dir(&mut self) -> bool {
        let Some(node_id) = self.current_node_id() else {
            return false;
        };
        let node = &self.nodes[node_id];
        if !node.is_dir {
            return false;
        }

        let path = node.path.clone();
        if self.expanded_dirs.contains(&path) {
            self.expanded_dirs.remove(&path);
        } else {
            self.expanded_dirs.insert(path.clone());
        }
        self.rebuild_visible();
        self.set_cursor_to_path(&path);
        true
    }

    /// カーソル位置のディレクトリを折りたたむ関数
    pub(in crate::tui) fn collapse_current_dir(&mut self) -> bool {
        let Some(node_id) = self.current_node_id() else {
            return false;
        };
        let node = &self.nodes[node_id];
        if !node.is_dir {
            return false;
        }

        let path = node.path.clone();
        if !self.expanded_dirs.remove(&path) {
            return false;
        }

        self.rebuild_visible();
        self.set_cursor_to_path(&path);
        true
    }

    /// カーソル位置のディレクトリを展開する関数
    pub(in crate::tui) fn expand_current_dir(&mut self) -> bool {
        let Some(node_id) = self.current_node_id() else {
            return false;
        };
        let node = &self.nodes[node_id];
        if !node.is_dir {
            return false;
        }

        let path = node.path.clone();
        if !self.expanded_dirs.insert(path.clone()) {
            return false;
        }

        self.rebuild_visible();
        self.set_cursor_to_path(&path);
        true
    }

    /// 指定したパスを展開して表示させる関数
    fn reveal_path(&mut self, path: &Path) {
        let Some(&node_id) = self.path_to_node.get(path) else {
            return;
        };

        let mut cursor = Some(node_id);
        while let Some(id) = cursor {
            if let Some(parent) = self.nodes[id].parent {
                let parent_path = self.nodes[parent].path.clone();
                self.expanded_dirs.insert(parent_path);
                cursor = Some(parent);
            } else {
                cursor = None;
            }
        }

        self.rebuild_visible();
    }

    /// 指定したパスにカーソルを合わせる関数
    fn set_cursor_to_path(&mut self, path: &Path) {
        let Some(&node_id) = self.path_to_node.get(path) else {
            return;
        };
        if let Some(pos) = self.visible_nodes.iter().position(|&id| id == node_id) {
            self.cursor_visible_index = pos;
        }
    }

    /// 画像ファイルのパスにカーソルを同期させる関数
    pub(in crate::tui) fn sync_cursor_to_image(&mut self, image_path: &Path) {
        self.reveal_path(image_path);
        self.set_cursor_to_path(image_path);
    }

    /// 描画用のエントリを生成する関数
    pub(in crate::tui) fn render_entries(
        &self,
        current_image_path: Option<&PathBuf>,
    ) -> Vec<FileTreeEntry> {
        self.visible_nodes
            .iter()
            .enumerate()
            .map(|(visible_index, &node_id)| {
                let node = &self.nodes[node_id];
                FileTreeEntry {
                    name: node.name.clone(),
                    depth: node.depth,
                    is_dir: node.is_dir,
                    is_expanded: node.is_dir && self.expanded_dirs.contains(&node.path),
                    is_cursor: visible_index == self.cursor_visible_index,
                    is_current_image: !node.is_dir
                        && current_image_path
                            .map(|path| path == &node.path)
                            .unwrap_or(false),
                }
            })
            .collect()
    }
}

/// ディレクトリ構造を再帰的に走査し、ツリー構造を構築する関数
pub(super) fn build_tree_nodes(
    dir: &Path,
    parent: Option<usize>,
    depth: usize,
    nodes: &mut Vec<TreeNode>,
    path_to_node: &mut HashMap<PathBuf, usize>,
    extensions: &[String],
) -> Vec<usize> {
    let mut dirs = Vec::new();
    let mut files = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            dirs.push(path);
        } else if is_image_path(&path, extensions) {
            files.push(path);
        }
    }

    dirs.sort_by(|a, b| natural_compare_paths_by_name(a, b));
    files.sort_by(|a, b| natural_compare_paths_by_name(a, b));

    let mut children = Vec::new();

    for path in dirs {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let node_id = nodes.len();
        nodes.push(TreeNode {
            path: path.clone(),
            name,
            depth,
            is_dir: true,
            parent,
            children: Vec::new(),
        });
        path_to_node.insert(path.clone(), node_id);
        let dir_children = build_tree_nodes(
            &path,
            Some(node_id),
            depth + 1,
            nodes,
            path_to_node,
            extensions,
        );
        nodes[node_id].children = dir_children;
        children.push(node_id);
    }

    for path in files {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown")
            .to_string();
        let node_id = nodes.len();
        nodes.push(TreeNode {
            path: path.clone(),
            name,
            depth,
            is_dir: false,
            parent,
            children: Vec::new(),
        });
        path_to_node.insert(path, node_id);
        children.push(node_id);
    }

    children
}

/// 描画用のエントリを生成する関数
#[derive(Debug, Clone)]
pub struct FileTreeEntry {
    pub name: String,
    pub depth: usize,
    pub is_dir: bool,
    pub is_expanded: bool,
    pub is_cursor: bool,
    pub is_current_image: bool,
}

/// ファイルツリーを描画する関数
fn truncate_with_ellipsis(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis = '…';
    let ellipsis_width = UnicodeWidthChar::width(ellipsis).unwrap_or(1);
    if max_width <= ellipsis_width {
        return ".".repeat(max_width);
    }

    let target_width = max_width - ellipsis_width;
    let mut out = String::new();
    let mut used_width = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used_width + w > target_width {
            break;
        }
        used_width += w;
        out.push(ch);
    }
    out.push(ellipsis);
    out
}

/// ファイルツリーを描画する関数
pub fn render_filetree(
    stdout: &mut io::Stdout,
    entries: &[FileTreeEntry],
    filetree_width: u16,
    term_height: u32,
) -> Result<()> {
    let rows = term_height.saturating_sub(2) as usize;
    let width = filetree_width as usize;
    let cursor_index = entries
        .iter()
        .position(|entry| entry.is_cursor)
        .unwrap_or(0);
    let start = if entries.len() <= rows {
        0
    } else {
        let half = rows / 2;
        cursor_index
            .saturating_sub(half)
            .min(entries.len().saturating_sub(rows))
    };

    for row in 0..rows {
        let y = (row as u16) + 1;
        let entry_index = start + row;
        let (line, is_cursor, is_current_image) = if let Some(entry) = entries.get(entry_index) {
            let depth_indent = " ".repeat(entry.depth.saturating_mul(2));
            let marker = if entry.is_dir {
                if entry.is_expanded { "- " } else { "+ " }
            } else {
                "  "
            };
            let cursor = if entry.is_cursor { ">" } else { " " };
            let display_name = if entry.is_dir {
                format!("{}/", entry.name)
            } else {
                entry.name.clone()
            };
            let raw = format!("{cursor}{depth_indent}{marker}{display_name}");
            (raw, entry.is_cursor, entry.is_current_image)
        } else {
            (String::new(), false, false)
        };

        let mut line = truncate_with_ellipsis(&line, width);
        let remains = width.saturating_sub(line.width());
        line.push_str(&" ".repeat(remains));

        if is_cursor {
            queue!(
                stdout,
                MoveTo(0, y),
                SetBackgroundColor(Color::DarkGrey),
                SetForegroundColor(Color::White),
                Print(line),
                ResetColor
            )?;
        } else if is_current_image {
            queue!(
                stdout,
                MoveTo(0, y),
                SetBackgroundColor(Color::DarkBlue),
                SetForegroundColor(Color::White),
                Print(line),
                ResetColor
            )?;
        } else {
            queue!(
                stdout,
                MoveTo(0, y),
                SetBackgroundColor(Color::Black),
                SetForegroundColor(Color::Grey),
                Print(line),
                ResetColor
            )?;
        }
    }

    Ok(())
}
