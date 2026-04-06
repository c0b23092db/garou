//! 画像描画の状態と入力パラメータを定義する。

use crate::model::config::{ImageDiffMode, TransportMode};
use std::{sync::Arc, time::Duration};

use super::transport::SharedMemoryState;

/// 差分更新に用いるRGBAフレーム
#[derive(Debug, Clone)]
pub struct RgbaFrame {
    /// 画像の幅
    pub width: u32,
    /// 画像の高さ
    pub height: u32,
    /// RGBAピクセルデータ
    pub pixels: Arc<[u8]>,
}

/// 画像描画時に必要な入力値
#[derive(Debug, Clone)]
pub struct ImageRenderParams {
    pub term_width: u32,
    pub available_height: u32,
    pub start_x: u16,
    pub always_upload: bool,
    pub transport_mode: TransportMode,
    pub diff_mode: ImageDiffMode,
    pub image_dimensions: (u32, u32),
    pub payload_hash: u64,
    pub image_data: Arc<[u8]>,
    pub encoded_payload: Arc<str>,
    pub prepared_upload_payload: Option<super::transport::UploadPayload>,
    pub refresh_image: bool,
    /// 画像の差分がどの程度存在していると差分更新ではなく完全再描画するかの閾値（0.0～1.0）
    pub dirty_ratio: f32,
    /// 差分判定タイルの一辺ピクセル数
    pub tile_grid: u32,
    /// 差分判定の画素間引き設定
    pub skip_step: u32,
    /// 画像表示ズーム倍率 (fit=1.0)
    pub zoom_factor: f32,
    /// 水平方向パン（セル単位）
    pub pan_x: i16,
    /// 垂直方向パン（セル単位）
    pub pan_y: i16,
    /// デコード済みRGBAフレーム（キャッシュ）
    pub rgba_frame: Option<RgbaFrame>,
}

/// 画像描画の差分判定に使う状態
#[derive(Debug, Default)]
pub struct ImageRenderState {
    /// 画像がすでにアップロードされているかどうか
    pub(super) has_uploaded: bool,
    /// 最後にアップロードした画像の内容のハッシュ値
    pub(super) last_payload_hash: Option<u64>,
    /// 最後に配置した画像の位置とサイズ (start_x, start_y, display_width_cells, display_height_cells)
    pub(super) last_placement: Option<(u16, u16, u32, u32)>,
    /// 最後にアップロード済みの RGBA フレーム（差分更新用）
    pub(super) last_rgba_frame: Option<RgbaFrame>,
    /// shared memory 転送時に生存期間を管理する保持領域
    pub(super) shared_memory: SharedMemoryState,
}

/// 画像描画で収集したメトリクス
#[derive(Debug, Clone, Copy)]
pub struct ImageRenderMetrics {
    pub render_duration: Duration,
    pub dirty_tiles: Option<usize>,
    pub placement: (u16, u16, u32, u32),
}

impl ImageRenderState {
    pub fn new() -> Self {
        Self::default()
    }

    pub(crate) fn reset_upload_state(&mut self) {
        self.has_uploaded = false;
        self.last_payload_hash = None;
        self.last_placement = None;
        self.last_rgba_frame = None;
    }
}
