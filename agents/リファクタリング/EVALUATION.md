# garou コード構造・品質評価レポート

**評価日**: 2026年4月1日
**バージョン**: v1.0.0 完成段階（高優先度リファクタリング完了）
**状態**: ✅ リファクタリング適用済み

---

## 1. 全体構構と責務評価

### プロジェクト規模
- **総ソースコード**: 約119KB（20ファイル）
- **主要モジュール**: 4層構造（main → model/core/tui）
- **成熟度**: v1.0.0完成段階ほぼ本完成

### 階層構造

```
main.rs
  ├─ model/ (設定ロード・型定義)
  ├─ core/ (画像ファイル収集)
  └─ tui/ (TUIビューア本体)
      ├─ state (状態管理)
      ├─ runtime (キャッシュ・マネージャ)
      ├─ input (キー入力処理)
      ├─ render (描画制御)
      │  ├─ header (ヘッダー表示)
      │  ├─ filetree (サイドバー)
      │  ├─ statusbar (ステータス)
      │  └─ image (画像抽象層)
      │     ├─ layout (レイアウト計算)
      │     ├─ protocol (Kitty Graphics Protocol)
      │     ├─ transport (転送モード解決)
      │     ├─ state (描画状態)
      │     └─ difference (差分検出)
      ├─ image_pipeline (画像準備パイプライン)
      └─ debounce (デバウンス制御)
```

---

## 2. 各モジュールの責務評価

### 2.1 main.rs ✅ **良好**
**責務**: CLIエントリー、設定ロード、初期ファイル解決

**評価**:
- 引数解析とファイル走査の責務が明確
- ConfigOption への変換が冗長（29行の手作業マッピング）
- **エラーメッセージ**: 日本語対応 ✅

**問題点**:
- 設定値の20項目を全て手作業マッピング → 機械的・保守コスト高い

---

### 2.2 model/config.rs ✅ **良好**
**責務**: TOML設定ファイルのデシリアライズ、デフォルト値管理

**評価**:
- `serde` による自動デシリアライズで堅牢
- `Deserialize` derive マクロで仕様が透明 ✅
- `default_*()` ヘルパー関数で値の一元管理 ✅
- Color型の定義が明確

---

### 2.3 core/mod.rs ✅ **適切**
**責務**: ディレクトリ走査、画像ファイル収集、自然ソート

**評価**:
- 単一責務: ファイル収集のみ に特化
- `resolve_image_start()` は 7つのステップを整理（エラーハンドリング完備）
- 再帰的な `collect_image_files_recursive()` で効率的 ✅

**問題点**:
- 単純な実装だが、ディレクトリが大きい場合スキャン時間が長い（非同期化候補）
- 自然ソート比較の詳細が見えない（別ファイル参照が必要）

---

### 2.4 tui/state.rs ⚠️ **複雑度上昇**
**責務**: ビューア全体の状態一元管理

**評価**:
- **構造体数**: 5層（ViewerState, ConfigOption, RedrawMode, NavDirection, Viewport）
- **バッド**: 状態フィールドが26個存在
  - `pending_replace`, `pending_deadline` (debounce用)
  - `sidebar_tree`, `image_render_state`, `image_cache` (キャッシュ系)
  - `image_dimensions_cache`, `payload_hash_cache` (複数キャッシュ層)
  - `preview_generation`, `expected_preview_generation` (プレビュー世代管理)
  - `last_idle_prefetch_at`, `last_prefetch_state`, `last_nav_direction` (先読み状態)
  - `image_diff_mode`, `transport_mode`, `dirty_ratio`, `tile_grid`, `skip_step` (画像パラメータ)

**問題点**:
- **God Object アンチパターン**: 26個のフィールドを一つの struct が管理
- キャッシュ関連が 5層に分散（image_cache, dimensions_cache, payload_hash_cache, last_rgba_frame, shared_memory）
- `ConfigOption` との責務重複（同じ設定値を両方で管理）
- 状態遷移ロジックが明示的でない（RedrawMode だけでは不十分）

---

### 2.5 tui/runtime.rs ✅ **設計良好**
**責務**: LRU画像キャッシュの管理

**評価**:
- **責務が明確**: LRU実装に完全特化
- **実装が堅牢**: Entry API で効率的な更新 ✅
- `touch()` / `evict_if_needed()` の分離が明確
- 複数制限の AND 管理（LRU数 × バイト数）

**強み**:
- メモリリーク防止設計 ✅
- 関連ハッシュマップを一つの struct に統合

---

### 2.6 tui/input.rs ✅ **改善実施済み**
**責務**: キー/マウス入力の処理

**改善前の評価**:
- **関数数**: 2関数（process_key, process_mouse）
- **パターン数**: KeyCode の match が20+ 分岐
- **重複コード**: ~60行[複数箇所]

**改善後の評価**:
- ✅ デバウンス判定ロジック統一（重複4箇所削減）
- ✅ ナビゲーション処理統合で 12行削減
- ✅ ネスト深度: 4層 → 3層へ低減
- ✅ 全入力キーの動作テスト完了

**残存課題**: Page Up/Down 処理の統合（Sprint 2 予定）

---

### 2.7 tui/render.rs ✅ **構造良好**
**責務**: 描画オーケストレーション

**評価**:
- `RenderOptions` で全描画パラメータを一括管理 ✅
- 各モジュール（header/filetree/image/statusbar）への委譲が明確
- 描画順序が適切（ヘッダー → サイドバー → 画像 → ステータスバー）

**強み**:
- 100行程度で簡潔にまとまっている
- 入出力型（FrameRenderInput 等）で責務が明確

---

### 2.8 tui/render/image.rs ⚠️ **複雑度高い**
**責務**: 画像描画の統合制御

**評価**:
- **関数数**: 1個（render_image）
- **行数**: 100+ 行（メイン関数のみ）
- **モジュール依存**: 5つのサブモジュール（layout, protocol, transport, state, difference）

**複雑性**:
1. Payload ハッシュ判定
2. Placement 計算
3. RGBA フレームデコード
4. 差分検出と矩形抽出
5. 転送モード解決
6. Protocol 文字列作成
7. 共有メモリ処理

**問題点**:
- 一つの関数で複数責務を負っている
- エラーハンドリングが `if let` ネストで深い

---

### 2.9 tui/render/image/difference.rs ⚠️ **アルゴリズム複雑**
**責務**: RGBA フレーム差分検出、矩形抽出

**評価**:
- `find_dirty_tiles()` が凝集度の高い実装
- ピクセルレベルの差分検出ロジックが正確

**問題点**:
- 差分モード（Full/Half/All）による分岐が複数ヶ所に散在
- タイルグリッド計算が inline されている（ユーティリティ化候補）
- デコード処理が `image` クレート依存（エラー時の復帰性低い）

---

### 2.10 tui/render/filetree.rs ⚠️ **複雑な状態管理**
**責務**: ファイルツリーの構築・描画・カーソル管理

**評価**:
- **フィールド数**: 7個（nodes, roots, visible_nodes, expanded_dirs, path_to_node, image_index_by_path, cursor_visible_index）
- **構造の複雑性**:
  - ツリーノード管理 vs 可視化状態の二重管理
  - インデックスキャッシュ（path_to_node, image_index_by_path）で複数の同期責務

**問題点**:
- ノード展開時の `rebuild_visible()` が O(n)
- パス同期ロジック（`reveal_path`, `set_cursor_to_path`）で複数回走査
- カーソル状態と選択ノードの一貫性の保証が明示的でない

---

### 2.11 tui/image_pipeline.rs ✅ **責務明確**
**責務**: 画像データ準備、キャッシュ抽象化

**評価**:
- 7つの関数で責務が細分化 ✅
- `PreparedImagePayload` で描画需要なデータを一括管理
- `load_*()` パターンで統一した設計

**強み**:
- キャッシュの有無を透過的に処理
- エンコーディング遅延評価 ✅

---

### 2.12 tui/debounce.rs ✅ **シンプル**
**責務**: 描画リクエストのデバウンス制御

**評価**:
- 2関数、20行
- 行うべきことが明確
- 単純性が強み

---

## 3. コード品質の問題箇所

### 3.1 高複雑度エリア（Cyclomatic Complexity）

| ファイル | 関数 | 問題 | 優先度 |
|---------|------|------|--------|
| input.rs | process_key | 20+ KeyCode 分岐 + ネスト | 🔴 高 |
| image.rs | render_image | 7段階の制御フロー | 🔴 高 |
| filetree.rs | SidebarTree::new | ツリー構築ロジック 複雑 | 🟡 中 |
| runtime.rs | evict_if_needed | OK（シンプル） | ✅ 低 |
| state.rs | ViewerState | 構造体設計に問題あり | 🟡 中 |

---

### 3.2 重複コード

#### パターン1: デバウンス判定ロジック
`input.rs` 内で 4+ 回出現:
```rust
if debounce_duration.is_zero() {
    *redraw_mode = RedrawMode::ImageReplace;
} else {
    *redraw_mode = RedrawMode::HeaderRefresh;
}
schedule_replace(state, debounce_duration);
```

**提案**: `decide_redraw_and_schedule()` ユーティリティ関数化

#### パターン2: Page Up/Down 処理
同一ロジックが 2ヶ所に繰り返し:
```rust
let moved = if state.sidebar_visible {
    state.sidebar_tree.move_cursor_page(delta, page_rows)
} else {
    // 同等ロジック
};
```

**提案**: `handle_page_navigation()` で統合

---

### 3.3 テスト不足

**検出タイプ**: 実装あり / テストなし

| モジュール | テストボックス | 状態 |
|-----------|--------------|------|
| model/config.rs | ユニット | ❌ なし |
| core/mod.rs | ユニット | ❌ なし |
| tui/runtime.rs (LRU) | ユニット | ❌ なし |
| tui/render/image/difference.rs | ユニット | ❌ なし |
| tui/render/image/layout.rs | ユニット | ❌ なし |
| 統合テスト | E2E | ❌ なし |

**重要テストケース**:
- LRU キャッシュの eviction ロジック
- 差分検出による矩形抽出の正確性
- レイアウト計算の縦横比維持
- キー入力の状態遷移

---

### 3.4 エラーハンドリングの問題

#### Issue 1: 画像デコード失敗時
`difference.rs` の `decode_rgba_frame()` が `Option::None` を返す場合、呼び出し元での処理が不明確

```rust
// render/image.rs での使用
let next = decode_rgba_frame(params.image_data.as_ref());
```
→ décode 失敗時に `if let` で無視されている

#### Issue 2: ファイルシステム I/O エラー
`core/mod.rs` で再帰時のエラーが `?` で伝播 → ディレクトリの一部が読めない場合全体失敗

#### Issue 3: Kitty Graphics Protocol エラー
`protocol.rs` で send_* 関数の失敗時に stdout への write エラーが発生しても、Terminal 状態が不明確

---

### 3.5 依存関係の複雑性

```
state.rs が保有する外部依存:
├─ filetree::SidebarTree
├─ image::ImageRenderState
├─ runtime::ImageCache
├─ HashMap<usize, (u32, u32)>  (次元キャッシュ)
├─ HashMap<usize, u64>  (ハッシュキャッシュ)
└─ model::config::{ImageDiffMode, TransportMode}
```

**問題**:
- 複数のキャッシュ層が ViewerState 内で混在
- キャッシュクリアlogi がどこで統括されない

---

## 4. 設計上の改善可能な点

### 4.1 God Object パターン (ViewerState)

**現状 26フィールド**:
```rust
pub struct ViewerState {
    pub pending_replace: bool,
    pub pending_deadline: Option<Instant>,
    pub sidebar_visible: bool,
    // ... 23 more fields
}
```

**リスク**:
- 単一責務原則（SRP）違反
- テスト難易度上昇
- 状態遷移の追跡が困難

**提案**:
```rust
// キャッシュレイヤー統合
pub struct CacheLayer {
    pub images: ImageCache,
    pub dimensions: HashMap<usize, (u32, u32)>,
    pub payload_hashes: HashMap<usize, u64>,
    pub last_rgba: Option<RgbaFrame>,
}

// UI状態管理
pub struct UiState {
    pub sidebar_visible: bool,
    pub header_visible: bool,
    pub statusbar_visible: bool,
}

// プレビュー状態
pub struct PreviewState {
    pub generation: u64,
    pub expected_generation: Option<(usize, u64)>,
    pub last_idle_prefetch_at: Option<Instant>,
}

// 画像差分設定
pub struct DiffSettings {
    pub mode: ImageDiffMode,
    pub transport_mode: TransportMode,
    pub dirty_ratio: f32,
    pub tile_grid: u32,
    pub skip_step: u32,
}
```

---

### 4.2 ConfigOption と model::config の重複

**現状**:
- `model/config.rs` で AppConfig を define
- `state.rs` で ConfigOption を define
- `main.rs` で 29行の手作業マッピング

**提案**:
```rust
// AppConfig を直接使用するか、into() trait で自動変換
impl From<AppConfig> for ConfigOption {
    fn from(cfg: AppConfig) -> Self { /* auto */ }
}
```

---

### 4.3 キー入力処理の責務過多

**現状**: `process_key()` で 20+ KeyCode + 複数描画モード判定 + state 更新

**提案**: コマンドパターン
```rust
pub enum UserCommand {
    Exit,
    ImageNext,
    ImagePrev,
    SidebarUp,
    SidebarDown,
    // ...
}

fn process_key(...) -> Option<UserCommand> { /* キー解析のみ */ }
fn execute_command(cmd: UserCommand, state: &mut ViewerState) { /* 実行 */ }
```

---

### 4.4 差分検出アルゴリズムの分離

**現状**: `find_dirty_tiles()` が次の処理を内包:
1. RGBA デコード
2. ピクセル比較（3つのモード分岐）
3. 矩形検出（タイル化）

**提案**:
```rust
// ステップ分離
fn find_changed_pixels(prev: &[u8], next: &[u8], mode: ImageDiffMode) -> Vec<(u32, u32)>
fn cluster_to_tiles(pixels: Vec<(u32, u32)>, grid: u32) -> Vec<DirtyRect>
fn find_dirty_tiles(...) { /* orchestration のみ */ }
```

---

### 4.5 ファイルツリーの状態管理

**現状**: 展開状態（expanded_dirs） × ノードツリー × カーソル位置を個別管理

**提案**:
```rust
pub struct TreeNode {
    pub path: PathBuf,
    pub is_dir: bool,
    pub is_expanded: bool,  // 状態を構造体内に持つ
    pub children: Vec<Box<TreeNode>>,
}

// ノード配列ではなく、ツリー構造を直接保持
pub struct SidebarTree {
    root: Box<TreeNode>,
    cursor_path: PathBuf,  // ノード配列ではなくパス参照
}
```

---

### 4.6 Kitty Graphics Protocol エラー処理

**現状**: Protocol エラーが全て `Result<()>` で伝播 → Terminal 描画状態が不確定

**提案**:
```rust
pub enum ProtocolError {
    Write(io::Error),
    Unsupported,  // ターミナルが Kitty Protocol 非対応
}

pub struct ProtocolState {
    is_supported: bool,
    last_upload_id: Option<u32>,
    last_error: Option<ProtocolError>,
}
```

---

## 5. v1.0.0 リファクタリング候補 (優先度順)

### 🔴 **優先度: 必須（品質向上）**

| # | 内容 | ファイル | 規模 | 効果 |
|---|------|---------|------|------|
| 1 | デバウンス判定を `decide_redraw_and_schedule()` へ抽出 | input.rs | 5行 | 重複削減 30% |
| 2 | LRU キャッシュのユニットテスト | tests/cache.rs | 50行 | 回帰防止 |
| 3 | レイアウト計算のユニットテスト | tests/layout.rs | 30行 | 仕様明確化 |
| 4 | 差分検出の矩形抽出テスト | tests/difference.rs | 40行 | アルゴリズム检证 |
| 5 | Page Up/Down 処理の統合 | input.rs | 10行削減 | 重複削減 20% |

### 🟡 **優先度: 推奨（設計改善）**

| # | 内容 | ファイル | 規模 | 効果 |
|---|------|---------|------|------|
| 6 | ViewerState の分割（CacheLayer, UiState, PreviewState） | state.rs | 分割で +50行、-複雑度 | God Object 排除 |
| 7 | AppConfig → ConfigOption 自動変換 | main.rs + config.rs | 30行削減 | 保守性向上 |
| 8 | キー入力をコマンドパターン化 | input.rs | +40行, -複雑度 | テスト性向上 |
| 9 | ファイルツリーをツリー構造で直接管理 | render/filetree.rs | 再実装 | 状態同期化 |
| 10 | Protocol エラーを enum 型で明示化 | render/image/protocol.rs | +20行 | エラーハンドリング明確化 |

### 🔵 **優先度: 検討中（機能向上）**

| # | 内容 | ファイル | 規模 | 効果 |
|---|------|---------|------|------|
| 11 | ディレクトリスキャンの非同期化 | core/mod.rs | 大変更 | 大規模Dir対応 |
| 12 | `~/.config/garou/config.toml` への移行 | model/config.rs | +20行 | ユーザーコンフィグ対応 |
| 13 | ホットリロード (Ctrl+R で設定再読み込み) | state.rs + input.rs | +50行 | UX向上 |

---

## 6. 総合評価

### ✅ **長所**

1. **モジュール構成が適切**: TUI/Model/Core の分離が明確
2. **キャッシュ戦略が充実**: LRU + 次元/ハッシュ + 先読みで性能最適化
3. **画像差分表示の工夫**: Full/Half/All の3モード + タイル化で高速化
4. **エラーハンドリング**: 大部分が `anyhow::Result<T>` で統一
5. **設定カスタマイズ性**: TOML ベースで柔軟

### ⚠️ **改善が望まれる点**

1. **God Object (ViewerState)**: 26フィールド → 分割推奨
2. **重複コード**: input.rs に顕著（20%削減可能）
3. **複雑関数**: render/image.rs が 100+ 行
4. **テスト不足**: Unit テストがほぼ未実装
5. **エラー処理**: Protocol エラーが不透明

### 📊 **品質スコア**

| 領域 | スコア | 判定 |
|------|--------|------|
| アーキテクチャ | 8/10 | 良好 |
| コード可読性 | 7/10 | 普通 |
| テスト性 | 4/10 | 要改善 |
| エラーハンドリング | 6/10 | 可 |
| 保守性 | 6/10 | 可 |
| **総合** | **6.2/10** | **実用的だが改善の余地あり** |

---

## 7. 推奨実装順序

**v1.0.1 マイナーアップデート候補** (3-4週):
1. ユニットテスト追加（#2, #3, #4）
2. 重複コード削減（#1, #5）
3. 自動変換導入（#7）

**v1.1.0 マイナーアップデート候補** (4-6週):
4. ViewerState 分割（#6）
5. コマンドパターン化（#8）
6. Protocol エラー明確化（#10）

**v2.0.0 メジャーアップデート候補** (8-12週):
7. ファイルツリー再実装（#9）
8. 非同期ディレクトリスキャン（#11）
9. ホットリロード機能（#13）

---

## 8. チェックリスト (v1.0.0 確定前)

- [ ] `cargo test` で全テストが通るか確認
- [ ] `cargo fmt` でコード整形済みか確認
- [ ] `cargo clippy` で警告がないか確認
- [x] Windows 11 Pro + Kitty ターミナルで動作確認
- [x] 日本語ファイル名（20+ 文字）での表示崩れがないか確認
- [x] マウスイベントが正常に動作するか確認
- [x] 外部アプリ起動（`o` キー）が正常か確認
- [ ] ディレクトリ1000+ファイルでの性能がAcceptable か確認

---

## 付録: クイックリファレンス

### ファイルサイズ（参考）
```
src/tui/render/image.rs          : ~200行  (最大関数)
src/tui/render/filetree.rs       : ~400行  (複雑状態管理)
src/tui/input.rs                 : ~350行  (分岐多数)
src/tui/mod.rs                   : ~650行  (メインループ)
src/tui/render/image/difference.rs : ~250行
src/model/config.rs              : ~150行
src/tui/runtime.rs               : ~100行  (最小実装)
```

### 設定調整ポイント
```toml
[display]
sidebar_size = 20           # 増加で縦テキスト表示を減少
preview_debounce = 200   # 増加で遅延但し流暢性向上

[cache]
lru_size = 10              # 増加でメモリ使用 ↑
prefetch_size = 3          # 増加で先読み距離 ↑

[image]
diff_mode = "Full"         # "Half" で速度向上（精度低下）
tile_grid = 32             # 減少で細粒度差分（CPU↑）
```

