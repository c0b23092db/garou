use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use crossterm::terminal::size as terminal_size;
use image::{DynamicImage, GenericImageView, ImageFormat, imageops::FilterType};
use std::{
    hash::{Hash, Hasher},
    io::Cursor,
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use super::render::image::{self as render_image, RgbaFrame, UploadPayload, UploadPixelFormat};
use crate::model::config::{ImageFilterType, TransportMode};

/// 画像の最大ピクセル数を定義する定数
const MAX_RGBA_DIFF_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_DIRECT_RGBA_UPLOAD_BYTES: usize = 512 * 1024;
fn terminal_pixel_limit() -> (u32, u32) {
    let (cols, rows) = terminal_size().unwrap_or((120, 40));
    let content_rows = rows.saturating_sub(2).max(1);
    let (cell_width, cell_height) = super::render::cell_pixel_size();

    let max_w = u32::from(cols).saturating_mul(cell_width);
    let max_h = u32::from(content_rows).saturating_mul(cell_height);
    (max_w.max(1), max_h.max(1))
}

/// 拡大表示用に画面サイズの `scale` 倍まで許す。
/// 拡大時は転送量が増えるので、画面サイズ以上なら image_width/height（0 は無制限）で頭打ちにする
fn scaled_limit((w, h): (u32, u32), scale: u32, (cap_w, cap_h): (u32, u32)) -> (u32, u32) {
    let cap = |base: u32, cap: u32| {
        let scaled = base.saturating_mul(scale.max(1));
        if cap == 0 {
            scaled
        } else {
            scaled.min(cap.max(base))
        }
    };
    (cap(w, cap_w), cap(h, cap_h))
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
    // image の resize は元画像の幅 x 縮小後の高さ分を f32 で畳み込むので、大きな画像では重い。
    // 先に整数倍の平均で縮めて、フィルタは最後の 2 倍未満の縮小だけに使う。
    let k = (w / target_w).min(h / target_h);
    box_shrink(image, k).resize_exact(target_w, target_h, filter)
}

/// k x k 画素の平均で 1/k に縮める（k < 2 なら何もしない）。
/// 端の k で割り切れない画素は捨てる（最大 k-1 画素）
fn box_shrink(image: DynamicImage, k: u32) -> DynamicImage {
    if k < 2 {
        return image;
    }
    let (w, h) = image.dimensions();
    let (out_w, out_h) = (w / k, h / k);
    match image {
        DynamicImage::ImageRgb8(img) => DynamicImage::ImageRgb8(
            image::RgbImage::from_raw(out_w, out_h, box_shrink_raw::<3>(&img, w, k)).unwrap(),
        ),
        other => DynamicImage::ImageRgba8(
            image::RgbaImage::from_raw(out_w, out_h, box_shrink_raw::<4>(&other.to_rgba8(), w, k))
                .unwrap(),
        ),
    }
}

/// C チャンネル 8bit の生データを k x k 平均で縮める
fn box_shrink_raw<const C: usize>(src: &[u8], width: u32, k: u32) -> Vec<u8> {
    let (k, src_w) = (k as usize, width as usize);
    let out_w = src_w / k;
    let area = (k * k) as u32;
    let mut out = Vec::with_capacity(out_w * (src.len() / (src_w * C * k)) * C);
    let mut sums = vec![[0u32; C]; out_w];
    for rows in src.chunks_exact(src_w * C * k) {
        sums.fill([0; C]);
        for row in rows.chunks_exact(src_w * C) {
            for (sum, block) in sums.iter_mut().zip(row.chunks_exact(k * C)) {
                for pixel in block.as_chunks::<C>().0 {
                    for (s, &v) in sum.iter_mut().zip(pixel) {
                        *s += u32::from(v);
                    }
                }
            }
        }
        out.extend(
            sums.iter()
                .flatten()
                .map(|&s| ((s + area / 2) / area) as u8),
        );
    }
    out
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
fn effective_transport_mode(
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
fn should_decode_rgba_frame(
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
#[allow(clippy::too_many_arguments)]
pub(super) fn prepare_image_payload(
    image_path: &Path,
    diff_mode: crate::model::config::ImageDiffMode,
    transport_mode: TransportMode,
    image_filter_type: ImageFilterType,
    file_width_limit: u32,
    file_height_limit: u32,
    allow_rgba_decode: bool,
    render_scale: u32,
) -> Result<PreparedImagePayload> {
    let started = Instant::now();
    let source_dims = ::image::image_dimensions(image_path).unwrap_or((1, 1));
    let transport_mode = effective_transport_mode(
        transport_mode,
        source_dims,
        file_width_limit,
        file_height_limit,
    );
    let (base_max_w, base_max_h) = scaled_limit(
        terminal_pixel_limit(),
        render_scale,
        (file_width_limit, file_height_limit),
    );
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
        let rgba_frame = if allow_rgba_decode && should_decode_rgba_frame(source_dims, diff_mode) {
            render_image::decode_rgba_payload(&bytes)
        } else {
            None
        };
        (Arc::from(bytes), source_dims, rgba_frame, false)
    };

    let encoded_payload = Arc::<str>::from(general_purpose::STANDARD.encode(image_data.as_ref()));
    let requested_transport = render_image::resolve_transport_mode(transport_mode);
    let prepared_upload_payload = match requested_transport {
        // file/temp_file はここでは書き出さず、転送直前に描画側で書く。
        // 先読みや準備済みキャッシュが同じファイルを上書きし、別の画像が表示されるのを防ぐ。
        render_image::ResolvedTransport::Direct if is_raw_rgba_direct => Some(UploadPayload {
            transport: render_image::ResolvedTransport::Direct,
            payload: encoded_payload.clone(),
            data_size: image_data.len(),
            pixel_format: UploadPixelFormat::Rgba,
            pixel_width: image_dimensions.0,
            pixel_height: image_dimensions.1,
        }),
        _ => None,
    };

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

/// 画像差分モードに応じて常にアップロードモードかどうかを判定する関数
pub(super) fn is_always_upload_mode(diff_mode: crate::model::config::ImageDiffMode) -> bool {
    matches!(diff_mode, crate::model::config::ImageDiffMode::All)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn box_shrink_averages_blocks() {
        // 4x2 の RGB を 2 倍縮小 → 左右それぞれ 2x2 ブロックの平均（端数は四捨五入）
        let px = |v: u8| [v, v, 255 - v];
        let raw: Vec<u8> = [0, 10, 100, 200, 20, 31, 100, 200, 9, 9]
            .into_iter()
            .take(8)
            .flat_map(px)
            .collect();
        let img = DynamicImage::ImageRgb8(image::RgbImage::from_raw(4, 2, raw).unwrap());
        let out = box_shrink(img, 2).to_rgb8();
        assert_eq!(out.dimensions(), (2, 1));
        assert_eq!(out.get_pixel(0, 0).0, [15, 15, 240]); // (0+10+20+31)/4 = 15.25
        assert_eq!(out.get_pixel(1, 0).0, [150, 150, 105]);
        // 大きな縮小でも最終サイズはフィルタ指定どおり
        let big = DynamicImage::ImageRgba8(image::RgbaImage::new(4000, 3000));
        let resized = resize_for_terminal(big, 960, 608, FilterType::Triangle);
        assert_eq!(resized.dimensions(), (811, 608));
    }

    #[test]
    fn scaled_limit_grows_with_zoom_up_to_cap() {
        assert_eq!(scaled_limit((960, 600), 1, (5120, 2880)), (960, 600));
        assert_eq!(scaled_limit((960, 600), 4, (5120, 2880)), (3840, 2400));
        assert_eq!(scaled_limit((960, 600), 8, (5120, 2880)), (5120, 2880));
        assert_eq!(scaled_limit((960, 600), 8, (0, 0)), (7680, 4800));
        // 上限が画面より小さくても画面サイズは下回らない
        assert_eq!(scaled_limit((960, 600), 2, (100, 100)), (960, 600));
    }
}
