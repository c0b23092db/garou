use crate::model::config::{ImageDiffMode, TransportMode};
use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub mod filetree;
pub mod header;
pub mod image;
pub mod overlay;
pub mod statusbar;

use self::{
    filetree::{FileTreeEntry, render_filetree},
    header::render_header,
    image::{
        ImageRenderMetrics, ImageRenderParams, ImageRenderState, RgbaFrame, UploadPayload,
        render_image, send_delete,
    },
    overlay::{build_overlay_info, render_overlay},
    statusbar::{StatusbarContent, render_statusbar},
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
    pub skip_image: bool,
    pub preserve_image: bool,
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
    /// Kitty Graphics Protocol で使用する image ID
    pub image_id: u32,
    /// index + payload_hash がIDキャッシュに存在するか
    pub id_cache_hit: bool,
    /// 元画像の幅と高さ（前処理前）
    pub source_dimensions: (u32, u32),
    /// 画像データのハッシュ値（描画の差分検出に使用）
    pub payload_hash: u64,
    /// 画像データの所有権を持つArc（描画関数に渡す際のクローンは軽量）
    pub image_data: Arc<[u8]>,
    /// 端末に送信するペイロードのエンコード後の文字列
    pub encoded_payload: Arc<str>,
    /// ワーカーで事前準備した転送payload（file/temp向け）
    pub prepared_upload_payload: Option<UploadPayload>,
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
    /// デコード済みRGBAフレーム（キャッシュ）
    pub rgba_frame: Option<RgbaFrame>,
    /// 画像情報オーバーレイの表示フラグ
    pub overlay_visible: bool,
    /// ステータスバーに表示するメッセージ
    pub status_message: Option<String>,
    /// 描画外で発生した画像処理時間（デコード/リサイズ/エンコード等）
    pub processing_duration: Duration,
}

/// 画面全体を描画する関数
pub fn render_frame(
    stdout: &mut io::Stdout,
    input: FrameRenderInput<'_>,
    options: RenderOptions,
    image_render_state: &mut ImageRenderState,
) -> Result<FrameRenderMetrics> {
    const CELL_PIXEL_WIDTH: u32 = 8;
    const CELL_PIXEL_HEIGHT: u32 = 16;

    let placement_to_pixel_size = |placement: (u16, u16, u32, u32)| -> (u32, u32) {
        (
            placement.2.saturating_mul(CELL_PIXEL_WIDTH),
            placement.3.saturating_mul(CELL_PIXEL_HEIGHT),
        )
    };

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
    let is_image_size_limit_error = options
        .status_message
        .as_deref()
        .is_some_and(|message| message == "Image size exceeds limit");

    let render_metrics = if options.skip_image {
        if is_image_size_limit_error || !options.preserve_image {
            if let Some(active_id) = image_render_state.active_image_id() {
                send_delete(stdout, active_id)?;
            }
            image_render_state.reset_upload_state();
            queue!(
                stdout,
                MoveTo(image_start_x, 1),
                Clear(ClearType::FromCursorDown)
            )?;
        }

        ImageRenderMetrics {
            render_duration: Duration::ZERO,
            dirty_tiles: None,
            placement: (image_start_x, 1, 0, 0),
        }
    } else {
        render_image(
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
                image_id: options.image_id,
                id_cache_hit: options.id_cache_hit,
                payload_hash: options.payload_hash,
                image_data: options.image_data,
                encoded_payload: options.encoded_payload,
                prepared_upload_payload: options.prepared_upload_payload,
                refresh_image: options.refresh_image,
                dirty_ratio: options.dirty_ratio,
                tile_grid: options.tile_grid,
                skip_step: options.skip_step,
                zoom_factor: options.zoom_factor,
                pan_x: options.pan_x,
                pan_y: options.pan_y,
                rgba_frame: options.rgba_frame,
            },
        )?
    };

    if let Some(message) = options.status_message.as_deref() {
        if is_image_size_limit_error {
            render_image_message_top_left(stdout, image_start_x, available_width, message)?;
        } else if !options.skip_image {
            // 待機メッセージで画像上に帯が残らないよう、skip_image中は中央帯を描かない。
            render_image_message_center(
                stdout,
                image_start_x,
                available_width,
                available_height,
                message,
            )?;
        }
    }

    // 画像描画後にサイドバーを重ねて、パン時の重なりを防ぐ。
    if options.sidebar_visible {
        let sidebar_width = options.sidebar_size.max(1);
        render_filetree(stdout, input.sidebar_entries, sidebar_width, term_height)?;
    }

    if options.statusbar_visible {
        if is_image_size_limit_error {
            render_statusbar(
                stdout,
                term_width,
                term_height,
                StatusbarContent {
                    elapsed: Duration::ZERO,
                    source_dimensions: options.source_dimensions,
                    rendered_dimensions: placement_to_pixel_size(render_metrics.placement),
                    status_message: None,
                },
                options.statusbar_bg_color,
                options.statusbar_fg_color,
            )?;
        } else if options.skip_image && options.status_message.is_none() {
            // 待機中にステータスバーを消さず、点滅を防ぐ。
        } else {
            let elapsed = render_metrics
                .render_duration
                .saturating_add(options.processing_duration);
            render_statusbar(
                stdout,
                term_width,
                term_height,
                StatusbarContent {
                    elapsed,
                    source_dimensions: options.source_dimensions,
                    rendered_dimensions: placement_to_pixel_size(render_metrics.placement),
                    status_message: options.status_message.as_deref(),
                },
                options.statusbar_bg_color,
                options.statusbar_fg_color,
            )?;
        }
    } else {
        queue!(
            stdout,
            MoveTo(0, term_height.saturating_sub(1) as u16),
            Clear(ClearType::CurrentLine)
        )?;
    }

    if options.overlay_visible
        && !options.skip_image
        && let Some(path) = input.image_files.get(input.current_index)
    {
        let info = build_overlay_info(path, options.source_dimensions, options.image_dimensions);
        render_overlay(stdout, term_width, term_height, &info)?;
    }

    stdout.flush()?;
    Ok(FrameRenderMetrics {
        render_duration: render_metrics.render_duration,
        dirty_tiles: render_metrics.dirty_tiles,
        placement: render_metrics.placement,
    })
}

fn render_image_message_center(
    stdout: &mut io::Stdout,
    image_start_x: u16,
    available_width: u32,
    available_height: u32,
    message: &str,
) -> Result<()> {
    if available_width == 0 || available_height == 0 {
        return Ok(());
    }

    let max_width = available_width.saturating_sub(2).max(1) as usize;
    let mut line = if message.width() > max_width {
        let mut out = String::new();
        let mut used_width = 0;
        for ch in message.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + ch_width > max_width {
                break;
            }
            used_width += ch_width;
            out.push(ch);
        }
        out
    } else {
        message.to_string()
    };

    let padding = max_width.saturating_sub(line.width());
    line.push_str(&" ".repeat(padding));

    let line_width = line.width() as u16;
    let centered_x =
        image_start_x.saturating_add((available_width as u16).saturating_sub(line_width) / 2);
    let centered_y = 1u16.saturating_add((available_height as u16) / 2);

    queue!(
        stdout,
        MoveTo(centered_x, centered_y),
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::White),
        Print(line),
        ResetColor
    )?;

    Ok(())
}

fn render_image_message_top_left(
    stdout: &mut io::Stdout,
    image_start_x: u16,
    available_width: u32,
    message: &str,
) -> Result<()> {
    if available_width == 0 {
        return Ok(());
    }

    let max_width = available_width.saturating_sub(2).max(1) as usize;
    let mut line = if message.width() > max_width {
        let mut out = String::new();
        let mut used_width = 0;
        for ch in message.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + ch_width > max_width {
                break;
            }
            used_width += ch_width;
            out.push(ch);
        }
        out
    } else {
        message.to_string()
    };

    let padding = max_width.saturating_sub(line.width());
    line.push_str(&" ".repeat(padding));

    queue!(
        stdout,
        MoveTo(image_start_x, 1),
        SetForegroundColor(Color::White),
        Print(line),
        ResetColor
    )?;

    Ok(())
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
