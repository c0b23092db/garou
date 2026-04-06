use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use crossterm::terminal::size as terminal_size;
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use std::{
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use super::{
    render::image::{self as render_image, RgbaFrame},
    state::{NavDirection, ViewerState},
};

/// 画像の最大ピクセル数を定義する定数
const MAX_RGBA_DIFF_PIXELS: u64 = 32 * 1024 * 1024;
const CELL_PIXEL_WIDTH: u32 = 8;
const CELL_PIXEL_HEIGHT: u32 = 16;
const DPI_SCALE_NUM: u32 = 1;
const DPI_SCALE_DEN: u32 = 1;
const RESIZE_TRIGGER_EDGE: u32 = 6000;
const RESIZE_TRIGGER_PIXELS: u64 = 48 * 1024 * 1024;

fn terminal_pixel_limit() -> (u32, u32) {
    let (cols, rows) = terminal_size().unwrap_or((120, 40));
    let content_rows = rows.saturating_sub(2).max(1);

    let max_w = u32::from(cols)
        .saturating_mul(CELL_PIXEL_WIDTH)
        .saturating_mul(DPI_SCALE_NUM)
        / DPI_SCALE_DEN;
    let max_h = u32::from(content_rows)
        .saturating_mul(CELL_PIXEL_HEIGHT)
        .saturating_mul(DPI_SCALE_NUM)
        / DPI_SCALE_DEN;

    (max_w.max(1), max_h.max(1))
}

fn should_resize_for_terminal(source_dims: (u32, u32), max_w: u32, max_h: u32) -> bool {
    let exceeds_terminal = source_dims.0 > max_w || source_dims.1 > max_h;
    if !exceeds_terminal {
        return false;
    }

    // 中サイズ画像は terminal 側スケーリングに任せて初回速度と画質を優先。
    let large_enough = source_dims.0 > RESIZE_TRIGGER_EDGE
        || source_dims.1 > RESIZE_TRIGGER_EDGE
        || image_pixel_count(source_dims) > RESIZE_TRIGGER_PIXELS;
    large_enough
}

fn resize_for_terminal(image: DynamicImage, max_w: u32, max_h: u32) -> DynamicImage {
    let (w, h) = image.dimensions();
    if w <= max_w && h <= max_h {
        return image;
    }

    let scale = (max_w as f32 / w as f32).min(max_h as f32 / h as f32).max(0.01);
    let target_w = ((w as f32 * scale).round() as u32).max(1);
    let target_h = ((h as f32 * scale).round() as u32).max(1);
    image.resize_exact(target_w, target_h, FilterType::Triangle)
}

fn image_pixel_count(image_dimensions: (u32, u32)) -> u64 {
    u64::from(image_dimensions.0).saturating_mul(u64::from(image_dimensions.1))
}

/// 画像のピクセル数が一定以下であればRGBAフレームをデコードして差分描画に利用する。大きな画像は常にペイロードハッシュを利用して差分描画する。
pub(super) fn should_decode_rgba_frame(
    image_dimensions: (u32, u32),
    diff_mode: crate::model::config::ImageDiffMode,
) -> bool {
    !matches!(diff_mode, crate::model::config::ImageDiffMode::All)
        && image_pixel_count(image_dimensions) <= MAX_RGBA_DIFF_PIXELS
}

/// 大きな画像のペイロードハッシュを計算する関数。画像全体を読み込まずに、ファイルサイズや更新日時などのメタデータを利用してハッシュを生成する。
fn large_image_payload_hash(image_path: &Path, image_dimensions: (u32, u32), image_data: &[u8]) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher = DefaultHasher::new();
    image_dimensions.hash(&mut hasher);
    image_data.len().hash(&mut hasher);

    if let Ok(metadata) = std::fs::metadata(image_path) {
        metadata.len().hash(&mut hasher);
        if let Ok(modified) = metadata.modified() {
            modified.hash(&mut hasher);
        }
    }

    hasher.finish()
}

/// ペイロードハッシュを読み込む関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_payload_hash(
    index: usize,
    image_path: &Path,
    image_data: &[u8],
    image_dimensions: (u32, u32),
    state: &mut ViewerState,
) -> u64 {
    if let Some(&cached) = state.payload_hash_cache().get(&index) {
        return cached;
    }

    let hash = if should_decode_rgba_frame(image_dimensions, state.image_diff_mode()) {
        render_image::hash_image_payload(image_data, state.image_diff_mode())
    } else {
        large_image_payload_hash(image_path, image_dimensions, image_data)
    };
    state.payload_hash_cache_mut().insert(index, hash);
    hash
}

#[derive(Debug, Clone)]
pub(super) struct PreparedImagePayload {
    pub(super) image_data: Arc<[u8]>,
    pub(super) image_dimensions: (u32, u32),
    pub(super) payload_hash: u64,
    pub(super) encoded_payload: Arc<str>,
    pub(super) rgba_frame: Option<RgbaFrame>,
    pub(super) prepare_duration: Duration,
}

/// ワーカースレッドで画像描画に必要なデータをまとめて準備する
pub(super) fn prepare_image_payload(
    image_path: &Path,
    diff_mode: crate::model::config::ImageDiffMode,
    _transport_mode: crate::model::config::TransportMode,
) -> Result<PreparedImagePayload> {
    let started = Instant::now();
    let source_dims = ::image::image_dimensions(image_path).unwrap_or((1, 1));
    let (max_w, max_h) = terminal_pixel_limit();
    let needs_resize = should_resize_for_terminal(source_dims, max_w, max_h);

    let bytes = std::fs::read(image_path)?;

    let (image_data, image_dimensions, rgba_frame): (Arc<[u8]>, (u32, u32), Option<RgbaFrame>) =
        if needs_resize {
            let decoded = ::image::load_from_memory(&bytes)?;
            let resized = resize_for_terminal(decoded, max_w, max_h);
            let resized_dims = resized.dimensions();

            // KGP f=100 で扱えるよう、縮小後は PNG に再エンコードする
            let mut png_bytes = Vec::new();
            {
                let mut cursor = Cursor::new(&mut png_bytes);
                resized.write_to(&mut cursor, ImageFormat::Png)?;
            }

            let rgba_frame = if should_decode_rgba_frame(resized_dims, diff_mode) {
                let rgba = resized.to_rgba8();
                Some(RgbaFrame {
                    width: resized_dims.0,
                    height: resized_dims.1,
                    pixels: Arc::from(rgba.into_raw()),
                })
            } else {
                None
            };

            (Arc::from(png_bytes), resized_dims, rgba_frame)
        } else {
            let image_data: Arc<[u8]> = Arc::from(bytes.clone());
            let rgba_frame = if should_decode_rgba_frame(source_dims, diff_mode) {
                render_image::decode_rgba_payload(&bytes)
            } else {
                None
            };
            (image_data, source_dims, rgba_frame)
        };

    let encoded_payload = Arc::<str>::from(general_purpose::STANDARD.encode(image_data.as_ref()));

    // ハッシュ計算
    let payload_hash = if matches!(diff_mode, crate::model::config::ImageDiffMode::All) {
        0
    } else if should_decode_rgba_frame(image_dimensions, diff_mode) {
        render_image::hash_image_payload(image_data.as_ref(), diff_mode)
    } else {
        large_image_payload_hash(image_path, image_dimensions, image_data.as_ref())
    };

    Ok(PreparedImagePayload {
        image_data,
        image_dimensions,
        payload_hash,
        encoded_payload,
        rgba_frame,
        prepare_duration: started.elapsed(),
    })
}

/// 画像データを読み込む関数。キャッシュが有効な場合はキャッシュを優先する。
pub(super) fn load_image_data(
    image_files: &[PathBuf],
    index: usize,
    state: &mut ViewerState,
) -> Result<Arc<[u8]>> {
    if !state.image_cache().enabled() {
        let bytes = std::fs::read(&image_files[index])?;
        return Ok(Arc::from(bytes));
    }

    if let Some(cached) = state.image_cache_mut().get(index) {
        state.record_cache_result(true);
        return Ok(cached);
    }

    state.record_cache_result(false);
    let data: Arc<[u8]> = Arc::from(std::fs::read(&image_files[index])?);
    state.image_cache_mut().insert(index, data.clone());
    Ok(data)
}

/// 画像の幅と高さを読み込む関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_image_dimensions(
    image_files: &[PathBuf],
    index: usize,
    state: &mut ViewerState,
) -> Result<(u32, u32)> {
    if let Some(&dims) = state.image_dimensions_cache().get(&index) {
        return Ok(dims);
    }

    let dims = ::image::image_dimensions(&image_files[index])?;
    state.image_dimensions_cache_mut().insert(index, dims);
    Ok(dims)
}

/// 画像差分モードに応じて常にアップロードモードかどうかを判定する関数
pub(super) fn is_always_upload_mode(diff_mode: crate::model::config::ImageDiffMode) -> bool {
    matches!(diff_mode, crate::model::config::ImageDiffMode::All)
}

/// 画像データをBase64エンコードして返す関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_encoded_payload(image_data: &[u8]) -> Arc<str> {
    Arc::<str>::from(general_purpose::STANDARD.encode(image_data))
}

/// RGBAフレームを取得する関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_rgba_frame(
    index: usize,
    image_data: &[u8],
    state: &mut ViewerState,
) -> Option<RgbaFrame> {
    if state.image_cache().enabled()
        && let Some(cached) = state.image_cache_mut().get_rgba(index)
    {
        return Some(cached);
    }

    let decoded = render_image::decode_rgba_payload(image_data)?;
    if state.image_cache().enabled() {
        state.image_cache_mut().insert_rgba(index, decoded.clone());
    }
    Some(decoded)
}

/// 隣接画像を先読みする関数
pub(super) fn prefetch_neighbors(
    image_files: &[PathBuf],
    current_index: usize,
    state: &mut ViewerState,
    max_steps: usize,
) {
    if !state.image_cache().enabled() || max_steps == 0 || image_files.len() < 2 {
        return;
    }

    if let Some((anchor_index, anchor_direction, anchor_depth)) = state.last_prefetch_state()
        && anchor_index == current_index
        && anchor_direction == state.last_nav_direction
        && anchor_depth >= max_steps
    {
        return;
    }

    let len = image_files.len();
    for step in 1..=max_steps {
        let step_mod = step % len;
        let primary = match state.last_nav_direction {
            NavDirection::Forward => (current_index + step_mod) % len,
            NavDirection::Backward => (current_index + len - step_mod) % len,
        };

        if primary == current_index || state.image_cache().contains(primary) {
            continue;
        }

        if let Ok(data) = std::fs::read(&image_files[primary]).map(Arc::from) {
            state.image_cache_mut().insert(primary, data);
        }
    }

    state.set_last_prefetch_state(Some((current_index, state.last_nav_direction, max_steps)));
}
