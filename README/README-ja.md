# Garou(がろう) - Simple Image Protocol Viewer for Kitty Graphics Protocol
```bash
siv.exe
```
**Kitty Graphics Protocolを使用した高速TUI画像ビューワー**

English - [README.md](../README.md)

## ⭐ 特徴
- **差分の高速表示**: 連続的な画像の差分表示を最適化
- **LRUキャッシング**: メモリ効率的な画像管理
- **デバウンス制御**: カーソル移動時のプレビュー更新を最適化
- **自然ソート**: ファイル名を1,2,3,10,11,12の順序で表示

## 💻 実行環境
### ターミナルエミュレータ
#### 検証済
- [x] Wezterm Nightly
#### 動作不能
- Windows Terminal

### OS
#### 検証済
- [x] Windows 11(64bit)
#### 未検証
- [ ] Linux
- [ ] Mac

## 📦 インストール

### cargo
#### `cargo install`
```bash
cargo install garou
```
#### `cargo binstall`
```bash
cargo binstall garou
```
#### `cargo install --git`
```bash
cargo install --git https://github.com/c0b23092db/garou
```

## 📖 コマンド
```
> siv --help
TUI: Simple Image Viewer for Kitty Graphics Protocol

Usage: siv.exe [PATH]

Arguments:
  [PATH]  Open Image file or Directory [defaults: current directory]

Options:
  -h, --help     Print help
  -V, --version  Print version
```

## ⌨️ 操作

### 常時
- **`Q / esc`**: 終了
- **`O`**: デフォルトプログラムで開く
- **`R`**: 画像のリフレッシュ
- **`Shift + R`**: 画面のリフレッシュ
- **`Alt + S`**: サイドバーの切り替え
- **`Alt + D`**: ステータスバーの切り替え
- **`Alt + F`**: ヘッダーの切り替え

### 画像表示
- **`H / ←`**: 前の画像へ移動
- **`L / →`**: 次の画像へ移動

### サイドバー
- **`J / ↓`**: カーソルを上に移動（即プレビュー）
- **`K / ↑`**: カーソルを下に移動（即プレビュー）
- **`G`**: 一番上に移動（即プレビュー）
- **`Shift + G`**: 一番下に移動（即プレビュー）
- **`Ctrl + B`**: 一ページ上に移動（即プレビュー）
- **`Ctrl + F`**: 一ページ下に移動（即プレビュー）
- **`H / ←`**: フォルダを折りたたむ
- **`L / →`**: フォルダを展開
- **`Enter`**: フォルダのトグル
- **左クリック**: ファイルを選択
- **ホイール**: カーソルを移動

### バグがある操作
- （preview）`0`: 画像のフィット
- （preview）`+`: 画像の拡大
- （preview）`-`: 画像の縮小
- （preview）`Shift + J`, `Shift + K`, `Shift + H`, `Shift + L`: 画像の移動

## ⚙️ 設定ファイル
`~/.config/garou/config.toml`を読み込みます。

```toml
[image]
extensions = ["png", "jpg", "jpeg", "gif", "webp", "bmp"]
diff_mode = "Full"
transport_mode = "auto"
dirty_ratio = 0.1
tile_grid = 32
skip_step = 1

[display]
sidebar = true
header = true
statusbar = true
sidebar_size = 20
preview_debounce = 100  # プレビュー更新のデバウンス（ミリ秒）
poll_interval = 10      # アイドル状態のポーリング間隔（ミリ秒）
prefetch_interval = 100 # アイドル状態の先読み間隔（ミリ秒）
header_bg_color = "dark_blue"    # ヘッダー背景色
header_fg_color = "white"        # ヘッダー文字色
statusbar_bg_color = "dark_gray" # ステータスバー背景色
statusbar_fg_color = "white"     # ステータスバー文字色

[cache]
lru_size = 10         # LRUキャッシュ最大数
prefetch_size = 1     # 先読みキャッシュ数
max_bytes = 268435456 # キャッシュ総容量上限（バイト）
```

### image

#### 画像表示プロセス（diff_mode）
- `All`: 差分判定を行わず、毎回画像をリフレッシュする
- `Full`: RGB（FFFFFF）のすべての番地を判定して変更がある場合のみ更新する
- `Half`: RGB（FFFFFF）の0番地、2番地、4番地のみ判定する

#### transport_mode(Kitty Graphics Protocolの転送モード)
- `auto`
- `direct`, `d`
- `file`, `f`
- `temp_file`, `t`
- `shared_memory`, `s`

##### autoの挙動
- Linux: `shared_memory` -> `direct`
- Windows: `direct`

#### 差分判定の閾値（dirty_ratio）
差分かどうかを判定する閾値です。0.0~1.0が設定できます。

#### 差分判定タイル（tile_grid）
変化判定に使うタイルの1辺ピクセル数です。

#### 画素間引き(skip_step)
指定したピクセル間隔で走査します。

### display

#### 起動時展開
起動時に開閉状態にする設定です。
- sidebar
- header
- statusbar

#### color
以下の色が選べます。
- black
- dark_gray
- gray
- white
- red
- dark_red
- green
- dark_green
- yellow
- dark_yellow
- blue
- dark_blue
- magenta
- dark_magenta
- cyan
- dark_cyan
- #RRGGBB（例: #1E90FF）
- rgb(r,g,b)（例: rgb(30,144,255)）

#### プレビュー更新のデバウンス（preview_debounce）
操作時のデバウンス時間をミリ秒で指定する。

#### アイドル状態のポーリング間隔（poll_interval）
ユーザーのキーボードやマウス操作をチェックするまでの最大待機時間をミリ秒で指定する。

#### アイドル状態の先読み間隔（prefetch_interval）
アイドル状態で隣の画像をプリフェッチする最小実行間隔をミリ秒で指定する。

### cache

#### max_bytes
`256 * 1024 * 1024`の**268435456**がデフォルト値となっています。

## おすすめ設定
### Windows(Local)
```toml
[image]
diff_mode = "All"
transport_mode = "file"

[display]
sidebar = true
header = true
statusbar = false
sidebar_size = 20
preview_debounce = 50

[cache]
lru_size = 5
prefetch_size = 1
```

## 認知しているバグ
- 大きい画像が開けない
- 画像のパンがうまく動作しない

## TODO
- **差分表示の高速表示**
- **サイズが大きい画像の高速表示**

- パフォーマンスメータ／統計表示（「直近の描画にかかった時間」「キャッシュヒット率」「diff 判定が走ったタイル数」など）
  - キャッシュヒット率・描画時間などの計測基盤は既に存在。statusbar/headerに追加表示するだけ。プロジェクトの性能重視ポリシーに揃う。
- ソート条件の切り替え
  - 現在は自然ソート固定。キー入力で「名前順→日付順→サイズ順」など切り替え可能。サイドバーツリーの再構築のみ。ファイルツリー側の変更のみで完結。
- 画像情報オーバーレイ
  - 幅・高さ・ファイルサイズ・形式などを画像上に重ねて表示。既存の Kitty Graphics Protocol 機能で対応可能
- キーバインドのユーザー定義
  - 現在は hardcoded。設定ファイル駆動に変更し、input.rs を設定参照に修正。競合検出処理は追加工数。リファクタリング投資。
- 簡易ファイル操作（削除・リネーム・別フォルダへ移動）
  - 削除・リネーム・移動の UI フロー設計が必要。サイドバー選択ファイルの操作+確認ダイアログ構築。
- ズーム・パン・フィット
  - Kitty Graphics Protocol の z パラメータで実現可能だが、TUI上のズーム率表示/パン入力方法の設計が必要。プロトコル仕様確認後に判断。

## 貢献
バグ報告、機能提案、プルリクエストを歓迎します。
agentsディレクトリに今後の展望などが書かれています。

## LICENSE
[MIT License](../LICENSE) / <http://opensource.org/licenses/MIT>

## 開発者
- ikata
