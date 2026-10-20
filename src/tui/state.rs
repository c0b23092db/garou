use crossterm::style::Color;
use std::{collections::HashSet, path::PathBuf, time::Duration, time::Instant};

use crate::model::config::{ImageDiffMode, ImageFilterType, TransportMode};
use crate::model::keymap::KeyMap;

use super::{
    image_pipeline::PreparedImagePayload,
    render::{filetree::SidebarTree, image::ImageRenderState},
    runtime::ImageCache,
};

/// アプリケーションの状態を管理する構造体
#[derive(Debug, Clone)]
pub struct ConfigOption {
    pub sidebar_visible: bool,
    pub header_visible: bool,
    pub statusbar_visible: bool,
    pub sidebar_size: u16,
    pub preview_debounce: u64,
    pub poll_interval: u64,
    pub prefetch_interval: u64,
    pub header_bg_color: Color,
    pub header_fg_color: Color,
    pub statusbar_bg_color: Color,
    pub statusbar_fg_color: Color,
    pub cache_lru_size: usize,
    pub cache_max_bytes: usize,
    pub prefetch_size: usize,
    pub image_diff_mode: ImageDiffMode,
    pub transport_mode: TransportMode,
    pub dirty_ratio: f32,
    pub tile_grid: u32,
    pub skip_step: u32,
    pub image_width: u32,
    pub image_height: u32,
    pub image_filter_type: ImageFilterType,
    pub image_extensions: Vec<String>,
    pub keymap: KeyMap,
}

/// 描画モードを表す層挙型
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RedrawMode {
    /// 何も描画しない
    Idle,
    /// ヘッダーのみ再描画
    HeaderRefresh,
    /// レイアウト全体を再描画
    FullRefresh,
    /// レイアウトの差分更新
    LayoutRefresh,
    /// 画像の差分更新
    ImageRefresh,
    /// 画像の完全再描画
    ImageReplace,
    /// 転送済み画像の配置のみ更新（ズーム・パン）
    ImagePlace,
}

/// ナビゲーションの方向を表す列挙型
#[derive(Debug, Clone, Copy)]
pub(super) struct Viewport {
    pub(super) width: u16,
    pub(super) height: u16,
}

/// ナビゲーションの方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NavDirection {
    /// 前方 (次の画像)
    Forward,
    /// 後方 (前の画像)
    Backward,
}

/// UI表示状態を管理する構造体
#[derive(Debug)]
pub(super) struct UiState {
    pub(super) sidebar_visible: bool,
    pub(super) sidebar_size: u16,
    pub(super) header_visible: bool,
    pub(super) statusbar_visible: bool,
    pub(super) overlay_visible: bool,
    pub(super) header_bg_color: Color,
    pub(super) header_fg_color: Color,
    pub(super) statusbar_bg_color: Color,
    pub(super) statusbar_fg_color: Color,
}

/// キャッシュ層を管理する構造体
#[derive(Debug)]
pub(super) struct CacheState {
    /// 描画準備済みの画像（パスで保持するため並べ替えでも消えない。端末サイズ変更で破棄）
    pub(super) prepared: ImageCache<PathBuf, PreparedImagePayload>,
    /// ワーカーへ先読みを依頼中のパス
    pub(super) prefetch_inflight: HashSet<PathBuf>,
    /// 準備済み画像の世代。端末サイズ変更で進め、古い先読み結果を捨てる
    pub(super) prepared_epoch: u64,
}

impl CacheState {
    /// 端末サイズに依存する準備済み画像を破棄する
    pub(super) fn invalidate_prepared(&mut self) {
        self.prepared.clear();
        self.prefetch_inflight.clear();
        self.prepared_epoch = self.prepared_epoch.wrapping_add(1);
    }
}

/// プレビュー・先読み状態を管理する構造体
#[derive(Debug)]
pub(super) struct PreviewState {
    pub(super) prefetch_size: usize,
    /// 最後に発行したプレビュー要求の世代番号
    pub(super) preview_generation: u64,
    /// 現在待機中のプレビュー要求 (index, generation)
    pub(super) expected_preview_generation: Option<(usize, u64)>,
    /// 最後にアイドル先読みを実行した時刻
    pub(super) last_idle_prefetch_at: Option<Instant>,
}

/// 画像処理設定を管理する構造体
#[derive(Debug, Clone)]
pub(super) struct ImageProcessingConfig {
    /// 画像の差分更新モード
    pub(super) image_diff_mode: ImageDiffMode,
    /// 画像転送モード
    pub(super) transport_mode: TransportMode,
    /// タイル差分送信を許可する最大面積比率 (0.0-1.0)
    pub(super) dirty_ratio: f32,
    /// 差分判定タイルの一辺ピクセル数
    pub(super) tile_grid: u32,
    /// 差分判定の画素間引き設定 (0,2,4)
    pub(super) skip_step: u32,
    /// File転送時の最大画像幅（0は無制限）
    pub(super) image_width: u32,
    /// File転送時の最大画像高さ（0は無制限）
    pub(super) image_height: u32,
    /// リサイズ時の補間フィルタ
    pub(super) image_filter_type: ImageFilterType,
    /// 画像表示のズーム倍率 (fit=1.0)
    pub(super) zoom_factor: f32,
    /// 拡大時に表示する中心（画像に対する正規化座標）
    pub(super) pan_center: (f32, f32),
    /// 転送画像の解像度倍率（画面に合わせた大きさの何倍で準備するか）
    pub(super) render_scale: u32,
}

/// パフォーマンス統計を管理する構造体
#[derive(Debug, Default)]
pub(super) struct PerformanceStats {
    pub(super) last_render_duration: Duration,
    pub(super) last_dirty_tiles: Option<usize>,
    pub(super) last_image_rect: Option<super::render::image::Placement>,
}

/// ビューワーの状態を管理する構造体
pub(super) struct ViewerState {
    /// デバウンス中の画像切り替えを実行する期限。None なら保留なし
    pub(super) pending_deadline: Option<Instant>,
    pub(super) ui_state: UiState,
    pub(super) cache: CacheState,
    pub(super) preview: PreviewState,
    pub(super) image_config: ImageProcessingConfig,
    pub(super) perf: PerformanceStats,
    pub(super) sidebar_tree: SidebarTree,
    pub(super) image_render_state: ImageRenderState,
    pub(super) last_nav_direction: NavDirection,
    pub(super) keymap: KeyMap,
    /// 画像ドラッグ中の直前のマウス位置 (列, 行)
    pub(super) drag_origin: Option<(u16, u16)>,
    /// 画面下部で受け付けている入力（開いている間はキーマップを使わない）
    pub(super) prompt: Option<Prompt>,
}

/// 画面下部の入力欄
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Prompt {
    /// ごみ箱へ移動する確認
    ConfirmDelete,
    /// 新しいファイル名を入力中
    Rename(String),
    /// 操作の失敗などを伝える（次のキーで閉じる）
    Message(String),
}

impl Prompt {
    pub(super) fn text(&self, current: Option<&std::path::Path>) -> String {
        let name = current
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy())
            .unwrap_or_default();
        match self {
            Self::ConfirmDelete => format!("ごみ箱へ移動しますか？ {name} (y/n)"),
            Self::Rename(input) => format!("新しい名前 (Enter: 確定 / Esc: 取消): {input}_"),
            Self::Message(message) => message.clone(),
        }
    }
}
