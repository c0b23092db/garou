use anyhow::Result;
use base64::{Engine as _, engine::general_purpose};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{
    render::image as render_image,
    state::{NavDirection, ViewerState},
};

#[derive(Debug, Clone)]
pub(super) struct PreparedImagePayload {
    pub(super) image_data: Arc<[u8]>,
    pub(super) image_dimensions: (u32, u32),
    pub(super) payload_hash: u64,
    pub(super) encoded_payload: Arc<str>,
}

/// ワーカースレッドで画像描画に必要なデータをまとめて準備する
pub(super) fn prepare_image_payload(
    image_path: &Path,
    diff_mode: crate::model::config::ImageDiffMode,
) -> Result<PreparedImagePayload> {
    let bytes = std::fs::read(image_path)?;
    let image_data: Arc<[u8]> = Arc::from(bytes);
    let image_dimensions = ::image::image_dimensions(image_path)?;
    let payload_hash = if matches!(diff_mode, crate::model::config::ImageDiffMode::All) {
        0
    } else {
        render_image::hash_image_payload(image_data.as_ref(), diff_mode)
    };
    let encoded_payload = Arc::<str>::from(general_purpose::STANDARD.encode(image_data.as_ref()));

    Ok(PreparedImagePayload {
        image_data,
        image_dimensions,
        payload_hash,
        encoded_payload,
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
        return Ok(cached);
    }

    let bytes = std::fs::read(&image_files[index])?;
    let data: Arc<[u8]> = Arc::from(bytes);
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

/// 画像データのハッシュを計算する関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_payload_hash(index: usize, image_data: &[u8], state: &mut ViewerState) -> u64 {
    if let Some(&cached) = state.payload_hash_cache().get(&index) {
        return cached;
    }

    let hash = render_image::hash_image_payload(image_data, state.image_diff_mode());
    state.payload_hash_cache_mut().insert(index, hash);
    hash
}

/// 画像差分モードに応じて常にアップロードモードかどうかを判定する関数
pub(super) fn is_always_upload_mode(diff_mode: crate::model::config::ImageDiffMode) -> bool {
    matches!(diff_mode, crate::model::config::ImageDiffMode::All)
}

/// 画像データをBase64エンコードして返す関数。キャッシュがあればキャッシュを優先する。
pub(super) fn load_encoded_payload(
    index: usize,
    image_data: &[u8],
    state: &mut ViewerState,
) -> Arc<str> {
    if state.image_cache_mut().enabled()
        && let Some(encoded) = state.image_cache_mut().get_encoded(index)
    {
        return encoded;
    }

    Arc::<str>::from(general_purpose::STANDARD.encode(image_data))
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

        if let Ok(bytes) = std::fs::read(&image_files[primary]) {
            let data: Arc<[u8]> = Arc::from(bytes);
            state.image_cache_mut().insert(primary, data);
        }
    }

    state.set_last_prefetch_state(Some((current_index, state.last_nav_direction, max_steps)));
}
