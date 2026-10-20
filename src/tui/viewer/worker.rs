use anyhow::Result;
use std::{collections::VecDeque, path::PathBuf, sync::mpsc, thread};

use super::super::image_pipeline::{PreparedImagePayload, prepare_image_payload};
use super::super::state::{NavDirection, ViewerState};
use crate::model::config::{ImageFilterType, TransportMode};

#[derive(Debug, Clone)]
pub(super) struct PreviewRequest {
    pub index: usize,
    /// 表示要求は preview_generation、先読みは prepared_epoch
    pub generation: u64,
    pub path: PathBuf,
    /// 先読み要求か（表示要求が優先される）
    pub prefetch: bool,
    pub diff_mode: crate::model::config::ImageDiffMode,
    pub transport_mode: TransportMode,
    pub image_filter_type: ImageFilterType,
    pub image_width_limit: u32,
    pub image_height_limit: u32,
    pub allow_rgba_decode: bool,
    pub render_scale: u32,
}

#[derive(Debug)]
pub(super) struct PreviewResponse {
    pub index: usize,
    pub generation: u64,
    pub path: PathBuf,
    pub prefetch: bool,
    pub payload: Result<PreparedImagePayload, String>,
}

/// index の画像の準備要求を組み立てる
fn build_request(
    image_files: &[PathBuf],
    index: usize,
    state: &ViewerState,
    generation: u64,
    prefetch: bool,
    allow_rgba_decode: bool,
) -> Option<PreviewRequest> {
    let path = image_files.get(index)?;
    Some(PreviewRequest {
        index,
        generation,
        path: path.clone(),
        prefetch,
        diff_mode: state.image_config.image_diff_mode,
        transport_mode: state.image_config.transport_mode,
        image_filter_type: state.image_config.image_filter_type,
        image_width_limit: state.image_config.image_width,
        image_height_limit: state.image_config.image_height,
        allow_rgba_decode,
        render_scale: state.image_config.render_scale,
    })
}

/// プレビュー準備リクエストをワーカーへ送信する
pub(super) fn submit_preview_request(
    tx: &mpsc::Sender<PreviewRequest>,
    image_files: &[PathBuf],
    index: usize,
    state: &mut ViewerState,
    force: bool,
) -> bool {
    if let Some((expected_index, _)) = state.preview.expected_preview_generation
        && expected_index == index
        && !force
    {
        return false;
    }
    if index >= image_files.len() {
        return false;
    }

    state.preview.preview_generation = state.preview.preview_generation.saturating_add(1);
    let generation = state.preview.preview_generation;
    state.preview.expected_preview_generation = Some((index, generation));
    let allow_rgba_decode = state.image_render_state.has_image() && !force;
    let sent = build_request(
        image_files,
        index,
        state,
        generation,
        false,
        allow_rgba_decode,
    )
    .is_some_and(|request| tx.send(request).is_ok());
    if !sent {
        state.preview.expected_preview_generation = None;
    }
    sent
}

/// 進行方向の隣接画像の描画準備をワーカーへ依頼する（準備済み・依頼中は除く）
pub(super) fn submit_prefetch_requests(
    tx: &mpsc::Sender<PreviewRequest>,
    image_files: &[PathBuf],
    current_index: usize,
    state: &mut ViewerState,
) {
    let len = image_files.len();
    if !state.cache.prepared.enabled() || len < 2 {
        return;
    }
    for step in 1..=state.preview.prefetch_size.min(len - 1) {
        let index = match state.last_nav_direction {
            NavDirection::Forward => (current_index + step) % len,
            NavDirection::Backward => (current_index + len - step) % len,
        };
        let path = &image_files[index];
        if state.cache.prepared.contains(path) || state.cache.prefetch_inflight.contains(path) {
            continue;
        }
        let allow_rgba_decode = state.image_render_state.has_image();
        let epoch = state.cache.prepared_epoch;
        let Some(request) =
            build_request(image_files, index, state, epoch, true, allow_rgba_decode)
        else {
            continue;
        };
        if tx.send(request).is_err() {
            return;
        }
        state
            .cache
            .prefetch_inflight
            .insert(image_files[index].clone());
    }
}

/// 次に処理する要求を選ぶ。最新の表示要求を優先して古い表示要求は捨て、先読みは古い順に処理する
fn next_request(queue: &mut VecDeque<PreviewRequest>) -> Option<PreviewRequest> {
    match queue.iter().rposition(|r| !r.prefetch) {
        Some(pos) => {
            let request = queue.remove(pos);
            queue.retain(|r| r.prefetch);
            request
        }
        None => queue.pop_front(),
    }
}

/// 画像プレビュー準備用のワーカースレッドを起動する
pub(super) fn spawn_preview_worker() -> (
    mpsc::Sender<PreviewRequest>,
    mpsc::Receiver<PreviewResponse>,
    thread::JoinHandle<()>,
) {
    let (req_tx, req_rx) = mpsc::channel::<PreviewRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<PreviewResponse>();

    let join = thread::spawn(move || {
        let mut queue = VecDeque::new();
        loop {
            if queue.is_empty() {
                match req_rx.recv() {
                    Ok(request) => queue.push_back(request),
                    Err(_) => break,
                }
            }
            queue.extend(req_rx.try_iter());
            let Some(request) = next_request(&mut queue) else {
                continue;
            };

            let payload = prepare_image_payload(
                &request.path,
                request.diff_mode,
                request.transport_mode,
                request.image_filter_type,
                request.image_width_limit,
                request.image_height_limit,
                request.allow_rgba_decode,
                request.render_scale,
            )
            .map_err(|e| e.to_string());
            if resp_tx
                .send(PreviewResponse {
                    index: request.index,
                    generation: request.generation,
                    path: request.path,
                    prefetch: request.prefetch,
                    payload,
                })
                .is_err()
            {
                break;
            }
        }
    });

    (req_tx, resp_rx, join)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(index: usize, prefetch: bool) -> PreviewRequest {
        PreviewRequest {
            index,
            generation: 0,
            path: PathBuf::new(),
            prefetch,
            diff_mode: crate::model::config::ImageDiffMode::All,
            transport_mode: TransportMode::Direct,
            image_filter_type: ImageFilterType::Nearest,
            image_width_limit: 0,
            image_height_limit: 0,
            allow_rgba_decode: false,
            render_scale: 1,
        }
    }

    #[test]
    fn latest_preview_beats_prefetch_and_older_previews() {
        let mut queue = VecDeque::from([
            request(1, true),
            request(2, false),
            request(3, true),
            request(4, false),
        ]);
        let order: Vec<usize> = std::iter::from_fn(|| next_request(&mut queue))
            .map(|r| r.index)
            .collect();
        assert_eq!(order, vec![4, 1, 3]);
    }
}

#[cfg(test)]
mod timing {
    use super::*;
    use crate::model::config::ImageDiffMode;
    use std::time::{Duration, Instant};

    fn median3(mut f: impl FnMut() -> Duration) -> Duration {
        let mut v = [f(), f(), f()];
        v.sort();
        v[1]
    }

    /// 隣接画像切替時にワーカーが行う準備処理の時間 (median of 3)。`cargo test --release -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn neighbor_switch_prepare_time() {
        let dir = std::env::temp_dir().join("garou_timing");
        std::fs::create_dir_all(&dir).unwrap();
        let big = dir.join("big.jpg");
        if !big.exists() {
            let img = image::RgbImage::from_fn(4000, 3000, |x, y| {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, ((x ^ y) % 256) as u8])
            });
            img.save(&big).unwrap();
        }
        for path in [PathBuf::from("README/png.png"), big] {
            let mut cache = crate::tui::runtime::ImageCache::<PathBuf, PreparedImagePayload>::new(
                10,
                256 * 1024 * 1024,
            );
            let prepare = median3(|| {
                let t = Instant::now();
                let payload = prepare_image_payload(
                    &path,
                    ImageDiffMode::All,
                    TransportMode::Direct,
                    ImageFilterType::Nearest,
                    5120,
                    2880,
                    false,
                    1,
                )
                .unwrap();
                let elapsed = t.elapsed();
                cache.insert(path.clone(), payload);
                elapsed
            });
            let cached = median3(|| {
                let t = Instant::now();
                cache.get(&path).unwrap();
                t.elapsed()
            });
            println!(
                "{}: prepare {:?} / cache hit {:?}",
                path.display(),
                prepare,
                cached
            );
        }
    }
}
