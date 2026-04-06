# AGENTS Instructions

最終更新: 2026-04-06
**v1.0.1 準備ステータス**: 🚧 高速化最優先ロードマップ実行中

## Project Goal
- Kitty Graphics Protocol を使った高速 TUI 画像ビューアーを実装する。
- 重要要件: カーソルを常に左起点で管理し、画像領域の変化を抑えつつカーソル移動を最小化する。画面幅にフィットさせた描画で縦横比を維持する。

## Product Summary
- 操作: `q`/`Esc` で終了、`h`/`l` または ←/→ で画像切替、`j`/`k` または ↑/↓ でサイドバー選択、`Enter` でディレクトリ展開/折りたたみまたは画像確定。
- 補助操作: `o/O` で外部アプリ起動、`r` で画像リフレッシュ、`R` でフルリフレッシュ、`Alt+S` でサイドバー表示切替、`b/B` でヘッダー/ステータス表示切替。
- 表示: 画像はヘッダー下の固定位置に送信し、`C=1` でカーソルを動かさず描画。ターミナル幅に合わせて縮小し縦横比を維持。
- 高速化方針: 差分モード（All/Full/Half）・デバウンス・キャッシュ/先読みを活用し、体感遅延を抑える。
- 対応前提: Kitty Graphics Protocol 対応ターミナル。

## Current Architecture
```
src/
  main.rs        - 引数パース、画像リスト取得、TUI 起動
  model/         - 設定ロードと型定義
  core/          - 画像ファイル収集などコア処理
  tui/
    ├─ state.rs          - ViewerState（責務分割済み）
    ├─ input.rs          - キー入力エントリ
    ├─ input/            - 移動/ズーム/パン/ソート/オープン処理
    ├─ render.rs         - 描画オーケストレーション
    ├─ runtime.rs        - LRU画像キャッシュ管理
    ├─ debounce.rs       - プレビュー更新デバウンス
    ├─ image_pipeline.rs - 画像準備パイプライン
    ├─ viewer/           - 描画ワーカー連携
    └─ render/
         ├─ image.rs         - 画像描画制御
         ├─ filetree.rs      - サイドバーツリー管理
         ├─ header.rs, statusbar.rs, overlay.rs - 補助UI描画
         └─ image/           - 差分検出・レイアウト・プロトコル・転送
```

## Rendering Rules
- 描画位置はヘッダー直下の固定点。カーソルは `SavePosition`/`RestorePosition` と `C=1` で移動を抑える。
- リサイズ: 端末幅を上限に横幅を決め、縦横比とセル比 (高さ:幅=2:1) を維持して高さを算出する。
- 文字列描画: ヘッダー/ステータスバー/ファイルツリーは Unicode 表示幅基準で切り詰め・余白埋めを行い、全角文字による折り返し崩れを防ぐ。
- 画面消去は最小限（全消去は必要時のみ）。ヘッダーと画像領域を分離。

## Development Policy
- 判断基準は `README.md` と `README/5W1H.md` を最優先、必要に応じて `README/要件.md` を参照する。
- 変更は最小単位で行い、無関係な設計変更を避ける。
- 設定キーは実装に合わせる（例: `display.sidebar_size`）。
- 実装後は `cargo check` を最低限実行する。
- **性能劣化の回避**: 変更前ベースラインを取得し、3回測定の中央値で評価する。2倍以上の性能低下は即時ロールバック。
- **遅延基準**: 描画時間が16ms〜24msを安定して超える場合は「極めて遅い」と判定して優先是正する。

## Current Priority (v1.0.1)
- A0 計測基盤固定: 描画時間・cache hit率・dirty tile数を定義し、表示フォーマットを統一する。
- A1 差分/転送最適化: terminal pixel情報、u32チャンク比較、セル倍数タイル整列、転送前リサイズを優先実装する。
- A2 非同期化: decode→resize→encode を `smol` ベースで非同期化し、RGBAキャッシュを統合する。
- A3 端末分岐: Wezterm/bcon/Kitty の経路分岐と ACK (`a=q`) 背圧制御を段階導入する。

## Environment Requirements
### Windows 11 Pro
- Rust (mise: latest)
- Kitty Graphics Protocol 対応ターミナル

## Standard Commands
- フォーマット: `cargo fmt`
- 静的確認: `cargo check`
- テスト: `cargo test`
- 手動確認: 日本語/全角を含む長いファイル名でヘッダー・サイドバーの表示崩れがないことを確認

## Completed Refactoring (Sprint 1: High Priority)

✅ **input.rs** - デバウンス判定ロジック統一、ナビゲーション処理統合
- 削減: ~60行（重複コード4箇所の統一）
- ネスト深度: 4層 → 3層
- テスト: 入力キー全動作確認

✅ **tui/state.rs** - God Object分割（26フィールド → 4つのsubstruct）
- UiState: UI可視性/色設定（8フィールド）
- CacheState: キャッシュ層統合（3フィールド）
- PreviewState: プレビュー・先読み（5フィールド）
- ImageProcessingConfig: 画像処理設定（5フィールド）
- 45個のアクセッサメソッド實装
- 全ファイル対応（mod.rs, input.rs, image_pipeline.rs）

⛔ **tui/render/image.rs分離**: 性能低下のため撤回（2026-04-01）
- 劣化: 1ms → 3ms（小画像3倍）、大画像30ms
- 原因: 差分なし時の不要再アップロード・再デコード
- 判定: 2倍以上低下は即ロールバック規則に従い復帰
  - 画像表示に16ms~24smを超える場合、**極めて動作が遅い**判定とする
- 状態: old_image.rs と同等の旧実装に完全復帰
- 教訓: 関数分割は読みやすさと性能のトレードオフ

## Backlog Notes (From README/今後のプロジェクト.md)
- ターミナルの pixel size 取得を描画入力に組み込み、セル/ピクセル換算を一元化する。
- 差分転送は dirty_ratio による2段階フォールバックを前提にする。
- 任意項目: `rayon` による並列差分判定、`memmap2` による I/O 最適化は A1/A2 の効果検証後に判断する。
- Simple Video Player 連携を見据え、FrameSource/FrameProcessor/FramePresenter 境界を維持する。

## References
- Project overview: @./README.md
- 5W1H: @./README/5W1H.md
- 要件: @./README/要件.md
- v1.0.0 Roadmap: @./REFACTORING_ROADMAP.md
- Project management: @./agents/projectmanagement.md
- Commands policy: @./agents/tools.md
- Rust skill: @./agents/Rust.md

## Related Agent Files
- Agent runtime guide: @./AGENTS.md
- Skill definition: @./SKILL.md
