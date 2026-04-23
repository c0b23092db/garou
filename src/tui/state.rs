use crossterm::style::Color;
use std::{collections::HashMap, time::Duration, time::Instant};

use crate::model::config::{ImageDiffMode, ImageFilterType, TransportMode};

use super::{
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
pub(super) struct CacheEntry {
    pub(super) image_dimensions: Option<(u32, u32)>,
    pub(super) payload_hash: Option<u64>,
    pub(super) metadata_hash: Option<u64>,
    pub(super) rgba_frame: Option<super::render::image::RgbaFrame>,
    pub(super) kitty_id: Option<(u32, u64)>,
}

/// キャッシュ層を管理する構造体
#[derive(Debug)]
pub(super) struct CacheState {
    pub(super) image_cache: ImageCache,
    pub(super) entries: HashMap<usize, CacheEntry>,
    pub(super) next_kitty_id: u32,
}

impl CacheState {
    pub(super) fn entry(&self, index: usize) -> Option<&CacheEntry> {
        self.entries.get(&index)
    }

    pub(super) fn entry_mut(&mut self, index: usize) -> &mut CacheEntry {
        self.entries.entry(index).or_insert_with(|| CacheEntry {
            image_dimensions: None,
            payload_hash: None,
            metadata_hash: None,
            rgba_frame: None,
            kitty_id: None,
        })
    }

    pub(super) fn cached_kitty_image_id(&self, index: usize, payload_hash: u64) -> Option<u32> {
        self.entry(index)
            .and_then(|entry| entry.kitty_id)
            .and_then(|(id, hash)| (hash == payload_hash).then_some(id))
    }

    pub(super) fn ensure_kitty_image_id(&mut self, index: usize, payload_hash: u64) -> (u32, bool) {
        if let Some(id) = self.cached_kitty_image_id(index, payload_hash) {
            return (id, true);
        }

        let mut next = self.next_kitty_id;
        if next == 0 {
            next = 1;
        }
        self.next_kitty_id = next.wrapping_add(1).max(1);
        self.entry_mut(index).kitty_id = Some((next, payload_hash));
        (next, false)
    }
}

/// プレビュー・先読み状態を管理する構造体
#[derive(Debug)]
pub(super) struct PreviewState {
    pub(super) prefetch_size: usize,
    pub(super) last_prefetch_state: Option<(usize, NavDirection, usize)>,
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
    /// 水平方向パン（セル単位）
    pub(super) pan_x: i16,
    /// 垂直方向パン（セル単位）
    pub(super) pan_y: i16,
}

/// パフォーマンス統計を管理する構造体
#[derive(Debug, Default)]
pub(super) struct PerformanceStats {
    pub(super) last_render_duration: Duration,
    pub(super) last_dirty_tiles: Option<usize>,
    pub(super) last_image_rect: Option<(u16, u16, u32, u32)>,
    pub(super) cache_requests: u64,
    pub(super) cache_hits: u64,
}

/// ビューワーの状態を管理する構造体
pub(super) struct ViewerState {
    pub(super) pending_replace: bool,
    /// 画像の差分更新が必要な期限。None の場合は差分更新不要。
    pub(super) pending_deadline: Option<Instant>,
    pub(super) ui_state: UiState,
    pub(super) cache: CacheState,
    pub(super) preview: PreviewState,
    pub(super) image_config: ImageProcessingConfig,
    pub(super) perf: PerformanceStats,
    pub(super) sidebar_tree: SidebarTree,
    pub(super) image_render_state: ImageRenderState,
    pub(super) last_nav_direction: NavDirection,
}
