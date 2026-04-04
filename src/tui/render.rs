use crate::model::config::{ImageDiffMode, TransportMode};
use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::Color,
    terminal::{Clear, ClearType},
};
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

pub mod filetree;
pub mod header;
pub mod image;
pub mod overlay;
pub mod statusbar;

use self::{
    filetree::{FileTreeEntry, render_filetree},
    header::render_header,
    image::{ImageRenderParams, ImageRenderState, render_image},
    overlay::{build_overlay_info, render_overlay},
    statusbar::render_statusbar,
};

#[derive(Debug, Clone, Copy)]
pub struct FrameRenderMetrics {
    pub render_duration: Duration,
    pub dirty_tiles: Option<usize>,
    pub placement: (u16, u16, u32, u32),
}

/// 描画に必要な入力データを管理する構造体
#[derive(Debug, Clone)]
pub struct FrameRenderInput<'a> {
    pub image_files: &'a [PathBuf],
    pub current_index: usize,
    pub sidebar_entries: &'a [FileTreeEntry],
    pub term_width: u16,
    pub term_height: u16,
}

/// ヘッダー描画の入力データを管理する構造体
#[derive(Debug, Clone)]
pub struct HeaderRenderInput<'a> {
    pub image_files: &'a [PathBuf],
    pub current_index: usize,
    pub sidebar_entries: &'a [FileTreeEntry],
    pub term_width: u16,
    pub term_height: u16,
    pub sidebar_visible: bool,
    pub header_visible: bool,
    pub sidebar_size: u16,
    pub header_bg_color: Color,
    pub header_fg_color: Color,
}

/// 描画オプションを管理する構造体
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub refresh_image: bool,
    pub full_refresh: bool,
    pub sidebar_visible: bool,
    pub header_visible: bool,
    pub statusbar_visible: bool,
    pub sidebar_size: u16,
    pub header_bg_color: Color,
    pub header_fg_color: Color,
    pub statusbar_bg_color: Color,
    pub statusbar_fg_color: Color,
    /// 画像データを常に転送するか（描画の差分検出を無効化して常に完全なペイロードを送る）
    pub always_upload: bool,
    /// 画像データの転送方法
    pub transport_mode: TransportMode,
    /// 差分比較方式
    pub diff_mode: ImageDiffMode,
    /// 画像の幅と高さ
    pub image_dimensions: (u32, u32),
    /// 画像データのハッシュ値（描画の差分検出に使用）
    pub payload_hash: u64,
    /// 画像データの所有権を持つArc（描画関数に渡す際のクローンは軽量）
    pub image_data: Arc<[u8]>,
    /// 端末に送信するペイロードのエンコード後の文字列
    pub encoded_payload: Arc<str>,
    /// タイル差分送信を許可する最大面積比率 (0.0-1.0)
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
    /// 画像情報オーバーレイの表示フラグ
    pub overlay_visible: bool,
    /// 画像キャッシュヒット率 (0.0-1.0)。キャッシュ無効時は None。
    pub cache_hit_rate: Option<f32>,
}

/// 画面全体を描画する関数
pub fn render_frame(
    stdout: &mut io::Stdout,
    input: FrameRenderInput<'_>,
    options: RenderOptions,
    image_render_state: &mut ImageRenderState,
) -> Result<FrameRenderMetrics> {
    let term_width = input.term_width as u32;
    let term_height = input.term_height as u32;

    if options.full_refresh {
        queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
    }

    if options.header_visible {
        render_header(
            stdout,
            input.image_files,
            input.current_index,
            term_width,
            options.header_bg_color,
            options.header_fg_color,
        )?;
    } else {
        queue!(stdout, MoveTo(0, 0), Clear(ClearType::CurrentLine))?;
    }
    let mut image_start_x = 0u16;
    let mut available_width = term_width;
    if options.sidebar_visible {
        let sidebar_width = options.sidebar_size.max(1);
        image_start_x = sidebar_width;
        available_width = term_width.saturating_sub(sidebar_width as u32);
    }

    let available_height = term_height.saturating_sub(2);

    let render_metrics = render_image(
        stdout,
        image_render_state,
        ImageRenderParams {
            term_width: available_width,
            available_height,
            start_x: image_start_x,
            always_upload: options.always_upload,
            transport_mode: options.transport_mode,
            diff_mode: options.diff_mode,
            image_dimensions: options.image_dimensions,
            payload_hash: options.payload_hash,
            image_data: options.image_data,
            encoded_payload: options.encoded_payload,
            refresh_image: options.refresh_image,
            dirty_ratio: options.dirty_ratio,
            tile_grid: options.tile_grid,
            skip_step: options.skip_step,
            zoom_factor: options.zoom_factor,
            pan_x: options.pan_x,
            pan_y: options.pan_y,
        },
    )?;

    // 画像描画後にサイドバーを重ねて、パン時の重なりを防ぐ。
    if options.sidebar_visible {
        let sidebar_width = options.sidebar_size.max(1);
        render_filetree(stdout, input.sidebar_entries, sidebar_width, term_height)?;
    }

    if options.statusbar_visible {
        render_statusbar(
            stdout,
            term_width,
            term_height,
            render_metrics.render_duration,
            options.image_dimensions,
            options.cache_hit_rate,
            render_metrics.dirty_tiles,
            options.statusbar_bg_color,
            options.statusbar_fg_color,
        )?;
    } else {
        queue!(
            stdout,
            MoveTo(0, term_height.saturating_sub(1) as u16),
            Clear(ClearType::CurrentLine)
        )?;
    }

    if options.overlay_visible
        && let Some(path) = input.image_files.get(input.current_index)
    {
        let info = build_overlay_info(path, options.image_dimensions);
        render_overlay(stdout, term_width, term_height, &info)?;
    }

    stdout.flush()?;
    Ok(FrameRenderMetrics {
        render_duration: render_metrics.render_duration,
        dirty_tiles: render_metrics.dirty_tiles,
        placement: render_metrics.placement,
    })
}

/// ヘッダーのみを描画する関数
pub fn render_header_only(stdout: &mut io::Stdout, input: HeaderRenderInput<'_>) -> Result<()> {
    if input.header_visible {
        render_header(
            stdout,
            input.image_files,
            input.current_index,
            input.term_width as u32,
            input.header_bg_color,
            input.header_fg_color,
        )?;
    } else {
        queue!(stdout, MoveTo(0, 0), Clear(ClearType::CurrentLine))?;
        queue!(
            stdout,
            MoveTo(0, (input.term_height as u32).saturating_sub(1) as u16),
            Clear(ClearType::CurrentLine)
        )?;
    }
    if input.sidebar_visible {
        let sidebar_width = input.sidebar_size.max(1);
        render_filetree(
            stdout,
            input.sidebar_entries,
            sidebar_width,
            input.term_height as u32,
        )?;
    }
    stdout.flush()?;
    Ok(())
}
