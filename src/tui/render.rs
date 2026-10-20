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
    sync::{Arc, OnceLock},
    time::Duration,
};
pub mod filetree;
pub mod header;
pub mod image;
pub mod message;
pub mod overlay;
pub mod statusbar;

use self::{
    filetree::{FileTreeEntry, render_filetree},
    header::render_header,
    image::{
        ImageRenderMetrics, ImageRenderParams, ImageRenderState, Placement, RgbaFrame,
        UploadPayload, render_image, render_placement, send_delete,
    },
    message::{render_image_message_center, render_image_message_top_left},
    overlay::{build_overlay_info, render_overlay},
    statusbar::{StatusbarContent, render_statusbar},
};

/// 画面全体の描画を管理するモジュール
#[derive(Debug, Clone, Copy)]
pub struct FrameRenderMetrics {
    pub render_duration: Duration,
    pub dirty_tiles: Option<usize>,
    pub placement: Placement,
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
    /// 拡大時に表示する中心（画像に対する正規化座標）
    pub pan_center: (f32, f32),
    /// デコード済みRGBAフレーム（キャッシュ）
    pub rgba_frame: Option<RgbaFrame>,
    /// 画像情報オーバーレイの表示フラグ
    pub overlay_visible: bool,
    /// ステータスバーに表示するメッセージ
    pub status_message: Option<String>,
    /// 描画外で発生した画像処理時間（デコード/リサイズ/エンコード等）
    pub processing_duration: Duration,
}

/// 起動時に CSI 16 t で問い合わせたセルのピクセルサイズ
static QUERIED_CELL_PIXEL_SIZE: OnceLock<(u32, u32)> = OnceLock::new();

/// OS からセルのピクセルサイズを得る（Windows では crossterm が未対応のため None）
fn window_cell_pixel_size() -> Option<(u32, u32)> {
    let size = crossterm::terminal::window_size().ok()?;
    if size.columns == 0 || size.rows == 0 {
        return None;
    }
    let width = u32::from(size.width) / u32::from(size.columns);
    let height = u32::from(size.height) / u32::from(size.rows);
    (width > 0 && height > 0).then_some((width, height))
}

/// 端末1セルのピクセルサイズ (幅, 高さ)。
/// OS → 起動時の問い合わせ結果 → 8x16 の順に使う。
pub fn cell_pixel_size() -> (u32, u32) {
    window_cell_pixel_size()
        .or_else(|| QUERIED_CELL_PIXEL_SIZE.get().copied())
        .unwrap_or((8, 16))
}

/// OS から取れない場合（Windows）に、端末へ CSI 16 t で問い合わせて結果を覚える。
/// raw mode 有効化後・イベントループ開始前に呼ぶ。応答がなければ短時間で諦める。
pub fn detect_cell_pixel_size(stdout: &mut io::Stdout) -> Result<()> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind};
    use std::time::Instant;

    if window_cell_pixel_size().is_some() {
        return Ok(());
    }
    write!(stdout, "\x1b[16t")?;
    stdout.flush()?;

    // 応答 (ESC [ 6 ; 高さ ; 幅 t) は Windows ではキー入力として届く
    let deadline = Instant::now() + Duration::from_millis(150);
    let mut reply = String::new();
    while let Some(remaining) = deadline.checked_duration_since(Instant::now())
        && event::poll(remaining)?
    {
        if let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && let KeyCode::Char(c) = key.code
        {
            reply.push(c);
            if c == 't' {
                break;
            }
        }
    }
    if let Some(size) = parse_cell_size_reply(&reply) {
        let _ = QUERIED_CELL_PIXEL_SIZE.set(size);
    }
    Ok(())
}

/// "[6;高さ;幅t" を含む応答から (幅, 高さ) を取り出す
fn parse_cell_size_reply(reply: &str) -> Option<(u32, u32)> {
    let body = reply.split_once("[6;")?.1;
    let (height, rest) = body.split_once(';')?;
    let width = rest.split_once('t')?.0;
    let (width, height) = (width.parse().ok()?, height.parse().ok()?);
    (width > 0 && height > 0).then_some((width, height))
}

/// 画像表示領域 (start_x, 幅, 高さ) を求める
fn image_area(
    term_width: u32,
    term_height: u32,
    sidebar_visible: bool,
    sidebar_size: u16,
) -> (u16, u32, u32) {
    let start_x = if sidebar_visible {
        sidebar_size.max(1)
    } else {
        0
    };
    (
        start_x,
        term_width.saturating_sub(u32::from(start_x)),
        term_height.saturating_sub(2),
    )
}

/// ズーム・パン後に画像の配置だけを更新する関数。転送済み画像がなければ None
pub fn render_image_placement(
    stdout: &mut io::Stdout,
    (term_width, term_height): (u16, u16),
    sidebar_visible: bool,
    sidebar_size: u16,
    zoom_factor: f32,
    pan_center: (f32, f32),
    image_render_state: &mut ImageRenderState,
) -> Result<Option<Placement>> {
    let (start_x, available_width, available_height) = image_area(
        u32::from(term_width),
        u32::from(term_height),
        sidebar_visible,
        sidebar_size,
    );
    render_placement(
        stdout,
        image_render_state,
        available_width,
        available_height,
        start_x,
        zoom_factor,
        pan_center,
    )
}

/// 画面全体を描画する関数
pub fn render_frame(
    stdout: &mut io::Stdout,
    input: FrameRenderInput<'_>,
    options: RenderOptions,
    image_render_state: &mut ImageRenderState,
) -> Result<FrameRenderMetrics> {
    let (cell_width, cell_height) = cell_pixel_size();
    let placement_to_pixel_size = |placement: Placement| -> (u32, u32) {
        (
            placement.cols.saturating_mul(cell_width),
            placement.rows.saturating_mul(cell_height),
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

    let (image_start_x, available_width, available_height) = image_area(
        term_width,
        term_height,
        options.sidebar_visible,
        options.sidebar_size,
    );
    let is_image_size_limit_error = options
        .status_message
        .as_deref()
        .is_some_and(|message| message == "Image size exceeds limit");

    let render_metrics = if options.skip_image {
        if is_image_size_limit_error || !options.preserve_image {
            if image_render_state.has_image() {
                send_delete(stdout)?;
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
            placement: Placement {
                x: image_start_x,
                y: 1,
                ..Placement::default()
            },
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
                payload_hash: options.payload_hash,
                image_data: options.image_data,
                encoded_payload: options.encoded_payload,
                prepared_upload_payload: options.prepared_upload_payload,
                refresh_image: options.refresh_image,
                dirty_ratio: options.dirty_ratio,
                tile_grid: options.tile_grid,
                skip_step: options.skip_step,
                zoom_factor: options.zoom_factor,
                pan_center: options.pan_center,
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

#[cfg(test)]
mod tests {
    use super::parse_cell_size_reply;

    #[test]
    fn parses_cell_size_reply() {
        assert_eq!(parse_cell_size_reply("\u{1b}[6;19;9t"), Some((9, 19)));
        assert_eq!(parse_cell_size_reply("[6;32;16t"), Some((16, 32)));
        assert_eq!(parse_cell_size_reply("[6;0;9t"), None);
        assert_eq!(parse_cell_size_reply("[6;19"), None);
        assert_eq!(parse_cell_size_reply(""), None);
    }
}
