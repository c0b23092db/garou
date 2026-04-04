use crossterm::style::Color;
use std::{collections::HashMap, time::Duration, time::Instant};

use crate::model::config::{ImageDiffMode, TransportMode};

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
pub(super) struct CacheState {
    pub(super) image_cache: ImageCache,
    pub(super) image_dimensions_cache: HashMap<usize, (u32, u32)>,
    pub(super) payload_hash_cache: HashMap<usize, u64>,
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

impl ViewerState {
    // ======================
    // UI State Accessors
    // ======================
    pub(super) fn sidebar_visible(&self) -> bool {
        self.ui_state.sidebar_visible
    }

    pub(super) fn set_sidebar_visible(&mut self, visible: bool) {
        self.ui_state.sidebar_visible = visible;
    }

    pub(super) fn header_visible(&self) -> bool {
        self.ui_state.header_visible
    }

    pub(super) fn set_header_visible(&mut self, visible: bool) {
        self.ui_state.header_visible = visible;
    }

    pub(super) fn statusbar_visible(&self) -> bool {
        self.ui_state.statusbar_visible
    }

    pub(super) fn set_statusbar_visible(&mut self, visible: bool) {
        self.ui_state.statusbar_visible = visible;
    }

    pub(super) fn overlay_visible(&self) -> bool {
        self.ui_state.overlay_visible
    }

    pub(super) fn set_overlay_visible(&mut self, visible: bool) {
        self.ui_state.overlay_visible = visible;
    }

    pub(super) fn sidebar_size(&self) -> u16 {
        self.ui_state.sidebar_size
    }

    pub(super) fn header_bg_color(&self) -> Color {
        self.ui_state.header_bg_color
    }

    pub(super) fn header_fg_color(&self) -> Color {
        self.ui_state.header_fg_color
    }

    pub(super) fn statusbar_bg_color(&self) -> Color {
        self.ui_state.statusbar_bg_color
    }

    pub(super) fn statusbar_fg_color(&self) -> Color {
        self.ui_state.statusbar_fg_color
    }

    // ======================
    // Cache Accessors
    // ======================
    pub(super) fn image_cache(&self) -> &ImageCache {
        &self.cache.image_cache
    }

    pub(super) fn image_cache_mut(&mut self) -> &mut ImageCache {
        &mut self.cache.image_cache
    }

    pub(super) fn image_dimensions_cache(&self) -> &HashMap<usize, (u32, u32)> {
        &self.cache.image_dimensions_cache
    }

    pub(super) fn image_dimensions_cache_mut(&mut self) -> &mut HashMap<usize, (u32, u32)> {
        &mut self.cache.image_dimensions_cache
    }

    pub(super) fn payload_hash_cache(&self) -> &HashMap<usize, u64> {
        &self.cache.payload_hash_cache
    }

    pub(super) fn payload_hash_cache_mut(&mut self) -> &mut HashMap<usize, u64> {
        &mut self.cache.payload_hash_cache
    }

    // ======================
    // Preview Accessors
    // ======================
    pub(super) fn prefetch_size(&self) -> usize {
        self.preview.prefetch_size
    }

    pub(super) fn last_prefetch_state(&self) -> Option<(usize, NavDirection, usize)> {
        self.preview.last_prefetch_state
    }

    pub(super) fn set_last_prefetch_state(&mut self, state: Option<(usize, NavDirection, usize)>) {
        self.preview.last_prefetch_state = state;
    }

    pub(super) fn preview_generation(&self) -> u64 {
        self.preview.preview_generation
    }

    pub(super) fn increment_preview_generation(&mut self) -> u64 {
        self.preview.preview_generation += 1;
        self.preview.preview_generation
    }

    pub(super) fn expected_preview_generation(&self) -> Option<(usize, u64)> {
        self.preview.expected_preview_generation
    }

    pub(super) fn set_expected_preview_generation(&mut self, generation: Option<(usize, u64)>) {
        self.preview.expected_preview_generation = generation;
    }

    pub(super) fn last_idle_prefetch_at(&self) -> Option<Instant> {
        self.preview.last_idle_prefetch_at
    }

    pub(super) fn set_last_idle_prefetch_at(&mut self, at: Option<Instant>) {
        self.preview.last_idle_prefetch_at = at;
    }

    // ======================
    // Image Config Accessors
    // ======================
    pub(super) fn image_diff_mode(&self) -> ImageDiffMode {
        self.image_config.image_diff_mode
    }

    pub(super) fn transport_mode(&self) -> TransportMode {
        self.image_config.transport_mode
    }

    pub(super) fn dirty_ratio(&self) -> f32 {
        self.image_config.dirty_ratio
    }

    pub(super) fn tile_grid(&self) -> u32 {
        self.image_config.tile_grid
    }

    pub(super) fn skip_step(&self) -> u32 {
        self.image_config.skip_step
    }

    pub(super) fn zoom_factor(&self) -> f32 {
        self.image_config.zoom_factor
    }

    pub(super) fn set_zoom_factor(&mut self, zoom_factor: f32) {
        self.image_config.zoom_factor = zoom_factor.clamp(0.1, 4.0);
    }

    pub(super) fn pan_x(&self) -> i16 {
        self.image_config.pan_x
    }

    pub(super) fn pan_y(&self) -> i16 {
        self.image_config.pan_y
    }

    pub(super) fn pan_by(&mut self, dx: i16, dy: i16) {
        self.image_config.pan_x = self.image_config.pan_x.saturating_add(dx);
        self.image_config.pan_y = self.image_config.pan_y.saturating_add(dy);
    }

    pub(super) fn set_pan(&mut self, pan_x: i16, pan_y: i16) {
        self.image_config.pan_x = pan_x;
        self.image_config.pan_y = pan_y;
    }

    pub(super) fn reset_pan(&mut self) {
        self.image_config.pan_x = 0;
        self.image_config.pan_y = 0;
    }

    pub(super) fn last_image_rect(&self) -> Option<(u16, u16, u32, u32)> {
        self.perf.last_image_rect
    }

    // ======================
    // Performance Accessors
    // ======================
    pub(super) fn record_render_metrics(
        &mut self,
        duration: Duration,
        dirty_tiles: Option<usize>,
        placement: (u16, u16, u32, u32),
    ) {
        self.perf.last_render_duration = duration;
        self.perf.last_dirty_tiles = dirty_tiles;
        self.perf.last_image_rect = Some(placement);
    }

    pub(super) fn record_cache_result(&mut self, hit: bool) {
        self.perf.cache_requests = self.perf.cache_requests.saturating_add(1);
        if hit {
            self.perf.cache_hits = self.perf.cache_hits.saturating_add(1);
        }
    }

    pub(super) fn cache_hit_rate(&self) -> Option<f32> {
        if self.perf.cache_requests == 0 {
            None
        } else {
            Some(self.perf.cache_hits as f32 / self.perf.cache_requests as f32)
        }
    }
}
