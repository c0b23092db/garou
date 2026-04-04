# Plan: README TODO統合の拡張ロードマップ

## 保存用の全体ステップ
1. Phase 0: ベースライン固定とTODO整理
2. 描画時間・キャッシュヒット率・差分タイル数の測定基準を固定する。全フェーズ共通ゲート
3. TODOをレーンA（性能/動画基盤）とレーンB（UX機能）へ分類し、依存関係を確定する。step 2と並列
4. Phase 1A: コア性能基盤（動画連携の前提）
5. terminal pixel情報を描画入力へ追加し、セル/ピクセル換算を一元化する。step 1依存
6. u32チャンク比較へ移行し、差分判定のホットループを最適化する。step 1依存
7. タイルグリッドをセル倍数に整列し、差分位置指定をセル精度で安定化する。step 5依存
8. image resizeを導入し、大画像の転送前縮小を標準経路化する。step 5依存
9. Phase 2A: 非同期パイプラインと転送戦略
10. decode→resize→encodeを非同期化し、UIスレッドの停止を解消する。step 8依存
11. RGBAキャッシュを非同期出力に統合して再デコードを抑止する。step 10依存
12. dirty_ratio 2段階フォールバックを実装する（少差分: MoveTo+a=T / 多差分: Alpha=0マスク+a=T）。step 7と11依存
13. 端末判定レイヤーでWezterm/bconとKittyの合成方式を分岐する（Kittyはa=fを選択可能化）。step 12依存
14. ACK（a=q）同期と背圧制御を追加し、60fps目標のフレーム整流を行う。step 10と12依存
15. Phase 3A: Simple Video Player連携境界
16. 責務境界を定義する（FrameSource / FrameProcessor / FramePresenter相当）。step 13と14依存
17. 静止画連番入力アダプタを作成し、将来の動画デコーダ差し替え点を固定する。step 16依存
18. 実動画デコードは本計画の対象外とし、連携APIまでを完了条件にする。step 17依存
19. Phase 1B: README TODOの低コストUX機能（Aと並走可能）
20. パフォーマンス統計表示をstatusbar/headerに追加する（直近描画時間・hit率・dirty tile数）。step 2後にstep 9と並走
21. ソート条件切替（名前/日付/サイズ）を追加し、サイドバーツリー再構築で反映する。step 20と並列
22. Phase 2B: README TODOの中コスト機能
23. ズーム・パン・フィットを追加する（入力設計 + レイアウト反映 + 表示倍率UI）。step 5と20依存
24. 画像情報オーバーレイを追加する（幅/高さ/サイズ/形式）。step 20と23依存
25. Phase 3B: README TODOの高コスト機能（後段）
26. キーバインドのユーザー定義を設定ファイル駆動に変更する（段階導入: 読込→適用→競合検出）。step 21依存
27. 簡易ファイル操作（削除/リネーム/移動）を確認フロー付きで追加する。step 21と26依存
28. Phase 4: 任意最適化
29. rayon差分並列化はA/Bベンチで優位を確認できた場合のみ採用する。step 12依存、optional
30. memmap2/代替デコーダは端末別ワークロードで有効性確認後に採用判断する。step 29と並列、optional

## plan.mdメモリ

将来拡張最優先の方針を維持しつつ、README-ja の TODO を同一計画に統合する。実装は 2 レーンで進める: レーンAはコア性能と動画連携基盤、レーンBはUX機能（統計表示・ソート・操作系）。Aの基盤依存を満たした後にBを段階投入し、常に性能ゲートで回帰を防ぐ。

**Steps**
1. Phase 0: ベースライン固定とTODO整理
2. 描画時間・キャッシュヒット率・差分タイル数の測定基準を固定する。*全フェーズ共通ゲート*
3. TODOをレーンA（性能/動画基盤）とレーンB（UX機能）へ分類し、依存関係を確定する。*parallel with step 2*
4. Phase 1A: コア性能基盤（動画連携の前提）
5. terminal pixel情報を描画入力へ追加し、セル/ピクセル換算を一元化する。*depends on 1*
6. u32チャンク比較へ移行し、差分判定のホットループを最適化する。*depends on 1*
7. タイルグリッドをセル倍数に整列し、差分位置指定をセル精度で安定化する。*depends on 5*
8. image resize を導入し、大画像の転送前縮小を標準経路化する。*depends on 5*
9. Phase 2A: 非同期パイプラインと転送戦略
10. decode→resize→encode を非同期化し、UIスレッドの停止を解消する。*depends on 8*
11. RGBAキャッシュを非同期出力に統合して再デコードを抑止する。*depends on 10*
12. dirty_ratio 2段階フォールバックを実装する（少差分: MoveTo+a=T / 多差分: Alpha=0マスク+a=T）。*depends on 7 and 11*
13. 端末判定レイヤーで Wezterm/bcon と Kitty の合成方式を分岐する（Kitty は a=f を選択可能化）。*depends on 12*
14. ACK（a=q）同期と背圧制御を追加し、60fps 目標のフレーム整流を行う。*depends on 10 and 12*
15. Phase 3A: Simple Video Player 連携境界
16. 責務境界を定義する（FrameSource / FrameProcessor / FramePresenter 相当）。*depends on 13 and 14*
17. 静止画連番入力アダプタを作成し、将来の動画デコーダ差し替え点を固定する。*depends on 16*
18. 実動画デコードは本計画の対象外とし、連携APIまでを完了条件にする。*depends on 17*
19. Phase 1B: README TODOの低コストUX機能（Aと並走可能）
20. パフォーマンス統計表示を statusbar/header に追加する（直近描画時間・hit率・dirty tile数）。*parallel with step 9 after step 2*
21. ソート条件切替（名前/日付/サイズ）を追加し、サイドバーツリー再構築で反映する。*parallel with step 20*
22. Phase 2B: README TODOの中コスト機能
23. ズーム・パン・フィットを追加する（入力設計 + レイアウト反映 + 表示倍率UI）。*depends on 5 and 20*
24. 画像情報オーバーレイを追加する（幅/高さ/サイズ/形式）。*depends on 20 and 23*
25. Phase 3B: README TODOの高コスト機能（後段）
26. キーバインドのユーザー定義を設定ファイル駆動に変更する（段階導入: 読込→適用→競合検出）。*depends on 21*
27. 簡易ファイル操作（削除/リネーム/移動）を確認フロー付きで追加する。*depends on 21 and 26*
28. Phase 4: 任意最適化
29. rayon差分並列化は A/B ベンチで優位を確認できた場合のみ採用する。*depends on 12, optional*
30. memmap2/代替デコーダは端末別ワークロードで有効性確認後に採用判断する。*parallel with step 29, optional*

**Relevant files**
- c:/Users/Admin/Documents/My_Program/Project/garou/README/todo.md — 既存性能課題の要件
- c:/Users/Admin/Documents/My_Program/Project/garou/README/今後のプロジェクト.md — 将来拡張と動画連携要件
- c:/Users/Admin/Documents/My_Program/Project/garou/README/README-ja.md — 追加TODO（統計・ソート・オーバーレイ等）
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render.rs — 描画入力集約（pixel情報受け渡し）
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/image_pipeline.rs — 非同期画像処理主経路
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render/image/difference.rs — 差分判定/u32比較/dirty tile計測
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render/image/layout.rs — fit/zoom/pan の配置算出
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render/image/protocol.rs — a=T/a=f/a=q の転送制御
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render/statusbar.rs — 統計表示の一次表示面
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render/header.rs — 補助統計表示とソートモード表示
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/render/filetree.rs — ソート切替時の再構築
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/input.rs — キー入力拡張（ソート/ズーム/将来キーバインド設定）
- c:/Users/Admin/Documents/My_Program/Project/garou/src/model/config.rs — キーバインド定義/表示設定拡張
- c:/Users/Admin/Documents/My_Program/Project/garou/src/core/mod.rs — 名前/日付/サイズソート関数
- c:/Users/Admin/Documents/My_Program/Project/garou/src/tui/state.rs — 統計値・ズーム状態・操作状態保持

**Verification**
1. 各フェーズ完了時に cargo check を実行する。
2. 小画像・大画像・連打時でベースライン比較し、2倍以上の劣化で当該フェーズをロールバックする。
3. 統計表示値（描画時間・hit率・dirty tile数）が実測と一致することを確認する。
4. ソート切替でファイル順が即時反映され、選択位置の破綻がないことを確認する。
5. ズーム/パン/フィットで縦横比保持と操作レスポンスを確認する。
6. 画像情報オーバーレイが画面崩れを起こさないことを確認する。
7. 端末分岐（Wezterm/bcon/Kitty）で転送経路が意図通りに選択されることを確認する。
8. ACK同期有効時にフレーム詰まりや過剰送信が抑制されることを確認する。
9. キーバインド設定で未定義/重複時の挙動が設計どおりであることを確認する。
10. 簡易ファイル操作で失敗時に安全に復帰し、ツリーとキャッシュが整合することを確認する。

**Decisions**
- 優先軸は将来拡張最優先のまま維持する。
- README-ja のTODOは別計画に分離せず、同一ロードマップへ統合する。
- 低コスト機能（統計表示・ソート）はコア性能レーンと並走で先行し、重機能（キーバインド定義・ファイル操作）は後段に配置する。
- 実動画デコード統合は除外し、連携可能なAPI境界を完了条件とする。

**Further Considerations**
1. オーバーレイはプロトコルレイヤー追加送信方式と画像焼き込み方式のどちらを標準化するか、性能優先で早期に固定する。
2. キーバインド競合検出は初期版を警告のみとし、禁止ルールは段階導入すると実装リスクを抑えられる。
3. ファイル操作は削除よりリネーム/移動を先行実装すると安全性検証がしやすい。