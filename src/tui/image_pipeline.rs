use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use crossterm::terminal::size as terminal_size;
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use std::{
    hash::{Hash, Hasher},
    io::Cursor,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

use super::{
    render::image::{
        self as render_image, RgbaFrame, UploadPayload, UploadPixelFormat,
        prepare_upload_payload_offthread,
    },
    state::{NavDirection, ViewerState},
};
use crate::model::config::{ImageFilterType, TransportMode};

/// 画像の最大ピクセル数を定義する定数
const MAX_RGBA_DIFF_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_DIRECT_RGBA_UPLOAD_BYTES: usize = 512 * 1024;
const CELL_PIXEL_WIDTH: u32 = 8;
const CELL_PIXEL_HEIGHT: u32 = 16;
const DPI_SCALE_NUM: u32 = 1;
const DPI_SCALE_DEN: u32 = 1;

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

fn resize_for_terminal(
    image: DynamicImage,
    max_w: u32,
    max_h: u32,
    filter: FilterType,
) -> DynamicImage {
    let (w, h) = image.dimensions();
    if w <= max_w && h <= max_h {
        return image;
    }

    let scale = (max_w as f32 / w as f32)
        .min(max_h as f32 / h as f32)
        .max(0.01);
    let target_w = ((w as f32 * scale).round() as u32).max(1);
    let target_h = ((h as f32 * scale).round() as u32).max(1);
    image.resize_exact(target_w, target_h, filter)
}

fn apply_transport_limit(
    max_w: u32,
    max_h: u32,
    transport_mode: TransportMode,
    file_width_limit: u32,
    file_height_limit: u32,
) -> (u32, u32) {
    if transport_mode != TransportMode::File {
        return (max_w, max_h);
    }

    let limited_w = if file_width_limit == 0 {
        u32::MAX
    } else {
        file_width_limit
    };
    let limited_h = if file_height_limit == 0 {
        u32::MAX
    } else {
        file_height_limit
    };
    // 0 は「上限なし」として解釈する。
    (limited_w.max(1), limited_h.max(1))
}

/// 画像の差分描画に常にペイロードハッシュを利用するモードかどうかを判定する関数
pub(super) fn effective_transport_mode(
    configured_mode: TransportMode,
    source_dims: (u32, u32),
    image_width_limit: u32,
    image_height_limit: u32,
) -> TransportMode {
    if configured_mode == TransportMode::Direct {
        return TransportMode::Direct;
    }

    let within_width = image_width_limit == 0 || source_dims.0 <= image_width_limit;
    let within_height = image_height_limit == 0 || source_dims.1 <= image_height_limit;

    if within_width && within_height {
        configured_mode
    } else {
        TransportMode::Direct
    }
}

/// 画像のピクセル数を計算する関数
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

/// 画像のメタデータから再生成判定用ハッシュを計算する。
fn image_metadata_hash(image_path: &Path, image_dimensions: (u32, u32)) -> u64 {
    use std::collections::hash_map::DefaultHasher;

    let mut hasher = DefaultHasher::new();
    image_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .hash(&mut hasher);
    image_dimensions.hash(&mut hasher);

    if let Ok(metadata) = std::fs::metadata(image_path) {
        metadata.len().hash(&mut hasher);
    }

    hasher.finish()
}

/// ペイロードハッシュを読み込む関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_payload_hash(
    index: usize,
    image_path: &Path,
    _image_data: &[u8],
    image_dimensions: (u32, u32),
    state: &mut ViewerState,
) -> u64 {
    if let Some(&cached) = state.payload_hash_cache().get(&index) {
        return cached;
    }

    let hash = image_metadata_hash(image_path, image_dimensions);
    state.payload_hash_cache_mut().insert(index, hash);
    hash
}

#[derive(Debug, Clone)]
pub(super) struct PreparedImagePayload {
    pub(super) image_data: Arc<[u8]>,
    pub(super) source_dimensions: (u32, u32),
    pub(super) image_dimensions: (u32, u32),
    pub(super) transport_mode: TransportMode,
    pub(super) prepared_upload_payload: Option<UploadPayload>,
    pub(super) payload_hash: u64,
    pub(super) encoded_payload: Arc<str>,
    pub(super) rgba_frame: Option<RgbaFrame>,
    pub(super) prepare_duration: Duration,
}

/// ワーカースレッドで画像描画に必要なデータをまとめて準備する
pub(super) fn prepare_image_payload(
    image_path: &Path,
    diff_mode: crate::model::config::ImageDiffMode,
    transport_mode: TransportMode,
    image_filter_type: ImageFilterType,
    file_width_limit: u32,
    file_height_limit: u32,
    allow_rgba_decode: bool,
) -> Result<PreparedImagePayload> {
    let started = Instant::now();
    let source_dims = ::image::image_dimensions(image_path).unwrap_or((1, 1));
    let transport_mode = effective_transport_mode(
        transport_mode,
        source_dims,
        file_width_limit,
        file_height_limit,
    );
    let (base_max_w, base_max_h) = terminal_pixel_limit();
    let (max_w, max_h) = apply_transport_limit(
        base_max_w,
        base_max_h,
        transport_mode,
        file_width_limit,
        file_height_limit,
    );
    let resize_filter = image_filter_type.as_filter_type();

    let bytes = std::fs::read(image_path)?;

    let (image_data, image_dimensions, rgba_frame, is_raw_rgba_direct): (
        Arc<[u8]>,
        (u32, u32),
        Option<RgbaFrame>,
        bool,
    ) = if source_dims.0 > max_w || source_dims.1 > max_h {
        let decoded = ::image::load_from_memory(&bytes)?;
        let resized = resize_for_terminal(decoded, max_w, max_h, resize_filter);
        let resized_dims = resized.dimensions();

        let rgba_upload_bytes = usize::try_from(
            u64::from(resized_dims.0)
                .saturating_mul(u64::from(resized_dims.1))
                .saturating_mul(4),
        )
        .unwrap_or(usize::MAX);

        if transport_mode == TransportMode::Direct
            && rgba_upload_bytes <= MAX_DIRECT_RGBA_UPLOAD_BYTES
        {
            let rgba = resized.to_rgba8();
            let raw = Arc::<[u8]>::from(rgba.into_raw());
            let rgba_frame =
                if allow_rgba_decode && should_decode_rgba_frame(resized_dims, diff_mode) {
                    Some(RgbaFrame {
                        width: resized_dims.0,
                        height: resized_dims.1,
                        pixels: raw.clone(),
                    })
                } else {
                    None
                };
            (raw, resized_dims, rgba_frame, true)
        } else {
            // file/temp/shared は互換性維持のため PNG ペイロードを継続利用する
            let mut png_bytes = Vec::new();
            {
                let mut cursor = Cursor::new(&mut png_bytes);
                resized.write_to(&mut cursor, ImageFormat::Png)?;
            }

            let rgba_frame =
                if allow_rgba_decode && should_decode_rgba_frame(resized_dims, diff_mode) {
                    let rgba = resized.to_rgba8();
                    Some(RgbaFrame {
                        width: resized_dims.0,
                        height: resized_dims.1,
                        pixels: Arc::from(rgba.into_raw()),
                    })
                } else {
                    None
                };

            (Arc::from(png_bytes), resized_dims, rgba_frame, false)
        }
    } else {
        let image_data: Arc<[u8]> = Arc::from(bytes.clone());
        let rgba_frame = if allow_rgba_decode && should_decode_rgba_frame(source_dims, diff_mode) {
            render_image::decode_rgba_payload(&bytes)
        } else {
            None
        };
        (image_data, source_dims, rgba_frame, false)
    };

    let encoded_payload = Arc::<str>::from(general_purpose::STANDARD.encode(image_data.as_ref()));
    let requested_transport = render_image::resolve_transport_mode(transport_mode);
    let prepared_upload_payload = match requested_transport {
        render_image::ResolvedTransport::File | render_image::ResolvedTransport::TempFile => {
            Some(prepare_upload_payload_offthread(
                requested_transport,
                encoded_payload.as_ref(),
                image_data.as_ref(),
            ))
        }
        render_image::ResolvedTransport::Direct if is_raw_rgba_direct => Some(UploadPayload {
            transport: render_image::ResolvedTransport::Direct,
            payload: encoded_payload.as_ref().to_string(),
            data_size: image_data.len(),
            pixel_format: UploadPixelFormat::Rgba,
            pixel_width: image_dimensions.0,
            pixel_height: image_dimensions.1,
        }),
        _ => None,
    };

    // ハッシュ計算
    let payload_hash = image_metadata_hash(image_path, image_dimensions);

    Ok(PreparedImagePayload {
        image_data,
        source_dimensions: source_dims,
        image_dimensions,
        transport_mode,
        prepared_upload_payload,
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
        && let Some(cached) = state.rgba_frame_cache().get(&index)
    {
        return Some(cached.clone());
    }

    let decoded = render_image::decode_rgba_payload(image_data)?;
    if state.image_cache().enabled() {
        state.rgba_frame_cache_mut().insert(index, decoded.clone());
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
