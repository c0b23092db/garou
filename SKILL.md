---
name: garou
description: Skills and practical guidance for Garou, a Kitty Graphics Protocol based TUI image viewer.
---

# SKILL

最終更新: 2026-04-06
**実装状態**: v1.0.1 準備中（高速化最優先フェーズ）

## Core Skills

### 1) Rust TUI 基本
- `crossterm` で raw mode / alternate screen / cursor 制御を行う。`KeyEventKind::Press` のみ処理し、キーリピート/リリースを無視して多重入力を防ぐ。
- Vim/yazi ライク入力: `q`/`Esc` 終了、`h`/`l`/←/→ で画像移動、`j`/`k`/↑/↓ でサイドバー選択、`Enter` で決定/展開切替。
- 補助入力: `o/O` 外部アプリ起動、`r` 画像リフレッシュ、`R` フルリフレッシュ、`Alt+S` サイドバー表示切替、`b/B` ヘッダー/ステータス表示切替。
- マウス左クリックでサイドバー行選択を受け付ける（サイドバー領域内のみ）。
- **状態管理**: ViewerState を 4つの責務別 struct に分割（UiState/CacheState/PreviewState/ImageProcessingConfig）し、45個のアクセッサメソッドで保守性向上。

### 2) Kitty Graphics Protocol 描画
- `a=T,f=100,C=1` でカーソルを動かさず PNG を送信する。描画前に `SavePosition`、後で `RestorePosition`。
- 画像はヘッダー直下の固定位置に送信し、ヘッダーと画像領域を分離して崩れを防ぐ。
- 画像再送方針: `refresh_image` 時は `a=d` で既存画像を削除し再アップロード、配置のみ変化時は `a=p` を使って再配置する。

### 3) スケーリング規則（max-width 相当）
- ターミナル幅を上限に横幅を決定し、縦横比とセル比 (高さ:幅=2:1) を維持して高さを算出する。
- 手順: `term_width, term_height` を取得 → `max_display_width = max(term_width-2, 1)` → `available_height = term_height - header領域` → `width_limit_by_height = available_height * aspect * cell_ratio` → `display_width = min(max_display_width, width_limit_by_height)` → `display_height = display_width / (aspect * cell_ratio)`。
- 0 回避のため `max(1, ...)` を徹底する。
- テキスト系 UI（ヘッダー/ステータスバー/サイドバー）は Unicode 表示幅で切り詰めと余白計算を行い、全角文字の折り返し崩れを防ぐ。

### 4) 画像・ファイル取り扱い
- 対応拡張子: png/jpg/jpeg/gif/webp/bmp。引数未指定時はカレントディレクトリから取得しソート。
- 設定は現状 `setting.toml`（プロジェクト直下）を読み込む。
- 設定キーは実装に従う（`display.sidebar_size`, `display.preview_debounce`, `cache.lru_size`, `cache.max_bytes`, `cache.prefetch_size`, `image.diff_mode`, `image.extensions`）。

### 5) パフォーマンスと安定性
- 全消去は必要最小限（`full_refresh` 時のみ全面クリア）。通常は差分方針で更新する。
- デバウンス付きプレビュー更新（`preview_debounce`）と LRU キャッシュ、近傍先読み（`prefetch_size`）で連続移動の待ち時間を抑える。
- 差分モード: `All`（常時再送）/`Full`（全バイト比較）/`Half`（間引き比較）を用途に応じて切替える。
- `dirty_ratio` による 2段階フォールバック（差分タイル上書き / フルフレーム送信）を前提に設計する。
- 描画時間が 16ms〜24ms を安定して超える場合は遅延異常として優先是正する。

### 6) 高速化ロードマップ実装スキル（v1.0.1）
- 大画像は転送前に `image` クレートでリサイズし、payload サイズを先に削減する。
- 差分判定ホットループは u32 チャンク比較を優先し、必要に応じて `skip_step` で比較密度を調整する。
- decode→resize→encode を `smol` ベースの非同期経路へ切り出し、UI スレッド停止を回避する。
- ACK (`a=q`) を用いた背圧制御を導入し、フレーム送出の詰まりを抑える。
- ターミナル特性に応じて転送方式を分岐する（Wezterm/bcon: `a=T`、kitty: `a=f` 検証）。

## Implementation Checklist
- 入力: raw mode + alternate screen のセットアップ/クリーンアップを対で呼ぶ。
- 描画: ヘッダー行 → `SavePosition` → Kitty 画像送信 → `RestorePosition` → `flush`。
- ナビゲーション: `KeyEventKind::Press` のみ判定。インデックスは 0..len-1 でラップ。
- サイドバー: カーソル移動時にファイルならプレビュー、ディレクトリなら展開/折りたたみを優先。
- 例外処理: 画像リスト空の場合は早期リターンし、メッセージを出力。

## Quality Checklist
- `cargo fmt`
- `cargo check`
- 変更前後で3回測定し中央値を比較（2倍以上の性能低下はロールバック）
- 手動: `q`/`Esc`/`h`/`l`/←/→/`j`/`k`/↑/↓/`Enter`/`o`/`r`/`R`/`Alt+S`/`b` の入力確認
- Kitty 対応ターミナルでの表示確認（幅フィットと縦横比維持を目視）
- 日本語や長いファイル名でヘッダー/サイドバーが折り返さず、画像領域に侵食しないことを確認

## Non-Goals
- GUI 化や Kitty 非対応端末向け描画は対象外。
- プロジェクト要件にない大規模アーキ変更は行わない。

## References
- `README.md`
- `README/5W1H.md`
- `README/要件.md`
- `REFACTORING_ROADMAP.md` — v1.0.0 完成後の改善計画
- `AGENTS.md`
- `agents/Rust.md`
