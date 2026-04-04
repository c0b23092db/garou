# v1.0.0 リファクタリング・ロードマップ

**作成日**: 2026年4月1日
**対象**: 完成段階での品質向上計画
**状態**: ✅ Sprint 1 完了、Sprint 2 開始準備中

---

## 1. 即時実施 (Sprint 1: 1～2週間) ✅ **COMPLETED**

### 1.1 🟢 **重複コード削減** - input.rs ✅ **完了**

**実施内容**: デバウンス判定ロジック統一、ナビゲーション処理統合

**成果**:
- **削減**: ~60行（重複4箇所を統一）
- **複雑度**: ネスト深度 4層 → 3層へ低減
- **テスト**: 全入力キーで動作確認 ✅

---

### 1.2 🟢 **ViewerState God Object 分割** - state.rs ✅ **完了**

**実施内容**: 26フィールドを4つの責務別 struct に分割

**成果**:
- **UiState** (8): sidebar_visible, header_visible, statusbar_visible, sidebar_size, header/statusbar 色
- **CacheState** (3): image_cache, image_dimensions_cache, payload_hash_cache
- **PreviewState** (5): prefetch_size, last_prefetch_state, preview_generation, expected_preview_generation, last_idle_prefetch_at
- **ImageProcessingConfig** (5): image_diff_mode, transport_mode, dirty_ratio, tile_grid, skip_step
- **ViewerState** (9): 上位マネジメント + pending_replace/deadline/sidebar_tree/image_render_state/last_nav_direction
- **アクセッサ**: 45個のメソッド實装
- **対応**: mod.rs, input.rs, image_pipeline.rs の全呼び出し箇所を更新
- **結果**: `cargo check` ✅ Pass

---

### 1.3 🟠 **tui/render/image.rs 分離** - ⛔ **撤回**（性能低下のため）

**試行内容**: 差分検出を3ステップ関数に分離（detect_patch_candidates, apply_patches, upload_full_payload）

**パフォーマンス劣化**:
- 小画像: 1ms → 3ms (3倍)
- 大画像: 15ms → 30ms以上 (2倍)

**根本原因**: 差分なし時に不要な再アップロード・再デコード発生

**判定基準**: "2倍以上の性能低下は即座にロールバック" → 適用 ✅

**復帰処理**:
- old_image.rs と同等の旧実装に完全復帰
- ボトルネック是正: パッチなしケースでの upload 呼び出し削除
- 検証: `cargo check` ✅ Pass

**教訓**: 関数分割は読みやすさと性能のトレードオフ。この案件では性能（ユーザー体感）を優先。

---
    let placement = compute_placement(100, 25, 0, (1920, 1080));
    // 縦横比が 16:9 = 1.78 であることを確認
    let computed_ratio = placement.1 as f32 / placement.2 as f32 * 2.0;
    assert!((computed_ratio - 1.78).abs() < 0.1);
}

#[test]
fn test_compute_placement_respects_boundaries() {
    let placement = compute_placement(80, 20, 0, (1920, 1080));
    assert!(placement.1 <= 80);   // term_width
    assert!(placement.2 <= 20);   // available_height
}
```

**テスト項目数**: 6個
**期待時間**: 20分

#### Cargo.toml に追加
```toml
[dev-dependencies]
tempfile = "3"  # 一時ファイルテスト用
```

**実装時間**: 1.5 時間
**検証**: `cargo test --lib` で全テスト PASS

---

### 1.3 🟡 **ConfigOption 自動変換** - main.rs + config.rs

**問題**: 29行の手作業マッピング
```rust
// main.rs 現在のコード
ConfigOption {
    sidebar_visible: config.display.sidebar,
    header_visible: config.display.header,
    // ... * 20
}
```

**解決策**: `From<AppConfig>` trait 実装

```rust
// config.rs に追加
impl From<AppConfig> for ConfigOption {
    fn from(cfg: AppConfig) -> Self {
        ConfigOption {
            sidebar_visible: cfg.display.sidebar,
            header_visible: cfg.display.header,
            // ... auto-generated
        }
    }
}

// main.rs 修正
let config_option = ConfigOption::from(config);
```

**実装時間**: 30分
**テスト**: `cargo check` で型確認

---

## 2. 次段階 (Sprint 2: 2～3週間)

### 2.1 🟡 **差分検出アルゴリズムの分離** - render/image/difference.rs

**現在**: `find_dirty_tiles()` 内に 3つのステップが凝集

**解決策**: ステップ分離

```rust
// Step 1: ピクセルレベルの変化検出
fn find_changed_pixels(
    prev: &RgbaFrame,
    next: &RgbaFrame,
    diff_mode: ImageDiffMode,
    skip_step: u32,
) -> Vec<(u32, u32)> {
    // 変化したピクセルの座標リスト返却
}

// Step 2: タイル化
fn cluster_pixels_to_tiles(
    pixels: Vec<(u32, u32)>,
    grid: u32,
    frame_width: u32,
    frame_height: u32,
) -> Vec<DirtyRect> {
    // grid サイズのタイルに分類
}

// Step 3: オーケストレーション
pub fn find_dirty_tiles(
    prev: &RgbaFrame,
    next: &RgbaFrame,
    diff_mode: ImageDiffMode,
    tile_grid: u32,
    skip_step: u32,
) -> Option<Vec<DirtyRect>> {
    let pixels = find_changed_pixels(prev, next, diff_mode, skip_step);
    if pixels.is_empty() {
        return Some(vec![]);  // 差分なし
    }
    let tiles = cluster_pixels_to_tiles(pixels, tile_grid, next.width, next.height);
    Some(tiles)
}
```

**テスト項目**:
- `test_find_changed_pixels_full_mode` (RGB全比較)
- `test_find_changed_pixels_half_mode` (RB比較)
- `test_cluster_pixels_to_tiles_no_duplicates`

**実装時間**: 2時間

---

### 2.2 🟡 **ファイルツリーの状態同期化** - render/filetree.rs

**問題**: 展開状態（expanded_dirs）× ノードツリー × カーソル位置の三重管理

**目下のコード**:
```rust
pub(crate) struct SidebarTree {
    nodes: Vec<TreeNode>,              // flat
    roots: Vec<usize>,
    visible_nodes: Vec<usize>,         // ビュー状態
    expanded_dirs: HashSet<PathBuf>,   // 展開状態
    cursor_visible_index: usize,       // カーソル
}
```

**解決策**: ツリー構造を直接保持

```rust
pub struct TreeNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,  // 状態を内包
    pub children: Vec<Box<TreeNode>>,
}

pub(crate) struct SidebarTree {
    root: Box<TreeNode>,
    cursor_path: PathBuf,  // ノードへの参照はパス
}

impl SidebarTree {
    fn toggle_expand(&mut self, path: &Path) {
        self.find_node_mut(path).map(|n| n.is_expanded ^= true);
    }

    fn sync_visible(&mut self) -> Vec<PathBuf> {
        // 展開状態に基づいて visible リストを再構築
    }
}
```

**マイグレーション**:
- `from_image_files()` 内部ロジック再実装
- `move_cursor()` → `cursor_path` 基準で変更
- `push_visible()` → `sync_visible()` で統一

**実装時間**: 4時間
**注意**: 大規模変更のため、既存テストで回帰確認必須

---

## 3. 中期計画 (Sprint 3: 3～4週間)

### 3.1 🟡 **ViewerState の分割** - state.rs

**現状の 26フィールド**を 4つの struct に分割

```rust
pub struct CacheLayer {
    pub images: ImageCache,
    pub dimensions: HashMap<usize, (u32, u32)>,
    pub payload_hashes: HashMap<usize, u64>,
    pub last_rgba: Option<RgbaFrame>,
    pub last_prefetch_state: Option<(usize, NavDirection, usize)>,
}

pub struct UiState {
    pub sidebar_visible: bool,
    pub header_visible: bool,
    pub statusbar_visible: bool,
    pub sidebar_size: u16,
    pub header_bg_color: Color,
    pub header_fg_color: Color,
    pub statusbar_bg_color: Color,
    pub statusbar_fg_color: Color,
    pub sidebar_tree: SidebarTree,
}

pub struct PreviewState {
    pub generation: u64,
    pub expected_generation: Option<(usize, u64)>,
    pub pending_replace: bool,
    pub pending_deadline: Option<Instant>,
    pub last_idle_prefetch_at: Option<Instant>,
}

pub struct DiffSettings {
    pub mode: ImageDiffMode,
    pub transport_mode: TransportMode,
    pub dirty_ratio: f32,
    pub tile_grid: u32,
    pub skip_step: u32,
}

pub struct ViewerState {
    pub cache: CacheLayer,
    pub ui: UiState,
    pub preview: PreviewState,
    pub diff: DiffSettings,
    pub image_render_state: ImageRenderState,
    pub last_nav_direction: NavDirection,
}
```

**影響範囲**: tui/mod.rs, input.rs, render.rs の全て

**マイグレーション戦略**:
1. 新 struct 定義
2. アクセスパスを `state.field` → `state.cache.field` へ更新
3. テスト実行確認
4. 旧コード削除

**実装時間**: 6 時間

---

### 3.2 🔵 **キー入力のコマンドパターン化** - input.rs

**目標**: input 処理をコマンド発行に分離 → テスト性向上

```rust
pub enum UserCommand {
    Exit,
    ImageNext,
    ImagePrev,
    SidebarUp,
    SidebarDown,
    OpenExternal,
    RefreshImage,
    FullRefresh,
    ToggleSidebar,
    ToggleHeader,
    ToggleStatusbar,
    PageUp,
    PageDown,
    Home,
    End,
    MouseClick(MouseEvent),
}

fn parse_key_to_command(key: KeyEvent, _term_height: u16) -> Option<UserCommand> {
    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Some(UserCommand::Exit),
        KeyCode::Char('h') | KeyCode::Left => Some(UserCommand::ImagePrev),
        KeyCode::Char('l') | KeyCode::Right => Some(UserCommand::ImageNext),
        // ...
    }
}

fn execute_command(
    cmd: UserCommand,
    image_files: &[PathBuf],
    current_index: &mut usize,
    redraw_mode: &mut RedrawMode,
    state: &mut ViewerState,
    debounce_duration: Duration,
    term_height: u16,
) -> bool {  // continue を返す
    match cmd {
        UserCommand::Exit => true,
        UserCommand::ImageNext => {
            *current_index = (*current_index + 1) % image_files.len();
            state.last_nav_direction = NavDirection::Forward;
            *redraw_mode = RedrawMode::ImageReplace;
            false
        }
        // ...
    }
}
```

**テスト例**:
```rust
#[test]
fn test_parse_key_exit() {
    let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
    assert_eq!(parse_key_to_command(key, 30), Some(UserCommand::Exit));
}
```

**実装時間**: 3 時間

---

### 3.3 🔵 **Protocol エラーの明確化** - render/image/protocol.rs

**現状**: `write!()` エラーが全て I/O エラーとして扱われる

```rust
pub enum ProtocolError {
    IoError(std::io::Error),
    UnsupportedByTerminal,
    InvalidPayload(String),
}

pub struct ProtocolState {
    is_supported: bool,
    last_uploaded_id: Option<u32>,
    last_error: Option<ProtocolError>,
}

pub fn send_upload(...) -> Result<(), ProtocolError> {
    // TerminalがProtocol非対応の場合を検出
    write!(stdout, ...)?
        .map_err(|e| ProtocolError::IoError(e))
}
```

**実装時間**: 1.5 時間

---

## 4. 将来計画 (v2.0.0+)

### 4.1 🔵 **ディレクトリスキャンの非同期化**
- 大規模ディレクトリ（10000+ ファイル）での応答時間短縮
- `tokio` or `smol` 既存依存で async impl
- 実装時間: 8-10 時間

### 4.2 🔵 **ホットリロード機能**
- `R` キーで `<current_dir>/.garou.toml` をリロード
- 表示設定を即座に反映
- 実装時間: 4-5 時間

---

## 5. テスト実行計画

### 5.1 ユニット テスト
```bash
cargo test --lib
```

### 5.2 統合 テスト（手動）
- [ ] Ctrl+B / Ctrl+F で Page Up/Down が動作するか
- [ ] Dir側のサイドバー展開/折り畳みが正常か
- [ ] 画像切り替え時に Redraw Mode が適切か
- [ ] 1000+ ファイルの場合の応答性

### 5.3 リグレッション テスト
- Windows 11 Pro + Kitty ターミナルで再テスト
- 日本語ファイル名（40文字以上）の表示確認
- マウスイベント全て確認

---

## 6. 実装チェックリスト

### 変更前
- [ ] 現在のコード `cargo check` で PASS
- [ ] 現在のコード `cargo test` で PASS
- [ ] 既存 branch から `git checkout -b refactor/xxx` で新規 branch 作成

### 変更後
- [ ] `cargo fmt` でコード整形
- [ ] `cargo clippy` で警告なし
- [ ] `cargo check` で PASS
- [ ] `cargo test` で全テスト PASS
- [ ] 手動テスト（上記参照）で動作確認
- [ ] `git diff` で意図しない変更がないか確認

---

## 参考: 実装ガイドライン

### Rust コーディング規約
- 関数名: `snake_case`
- 定数: `SCREAMING_SNAKE_CASE`
- モジュール: `snake_case`
- struct フィールド: `snake_case`
- Public API には `///` doc comments 必須

### テスト命名規約
```rust
#[test]
fn test_<unit>_<scenario>_<expected_result> {}
// 例: test_lru_cache_eviction_by_count
```

### コミット メッセージ テンプレート
```
<type>: <subject>

<body>

refs #<issue> (if applicable)
```

**type**: `refactor`, `feat`, `fix`, `test`
**subject**: 現在形、40文字以下

---

## 実装予定表

| Sprint | 期間 | タスク | 優先度 | 工数 |
|--------|------|--------|--------|------|
| 1 | 1-2w | 重複削減 + Test基盤 + Auto変換 | 🔴 | 3h |
| 2 | 2-3w | Page処理統合 + 差分分離 + ツリー同期 | 🟡 | 7h |
| 3 | 3-4w | State分割 + コマンドパターン + Protocol明確化 | 🟡 | 10h |
| **合計** | **1ヶ月** | **v1.0.1 → v1.1.0 準備** | | **20h** |

