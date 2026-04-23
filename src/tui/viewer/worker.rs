use anyhow::Result;
use std::{path::PathBuf, sync::mpsc, thread};

use super::super::image_pipeline::{PreparedImagePayload, prepare_image_payload};
use super::super::state::ViewerState;
use crate::model::config::{ImageFilterType, TransportMode};

#[derive(Debug, Clone)]
pub(super) struct PreviewRequest {
    pub index: usize,
    pub generation: u64,
    pub path: PathBuf,
    pub diff_mode: crate::model::config::ImageDiffMode,
    pub transport_mode: TransportMode,
    pub image_filter_type: ImageFilterType,
    pub image_width_limit: u32,
    pub image_height_limit: u32,
    pub allow_rgba_decode: bool,
}

#[derive(Debug)]
pub(super) struct PreviewResponse {
    pub index: usize,
    pub generation: u64,
    pub payload: Result<PreparedImagePayload, String>,
}

/// プレビュー準備リクエストをワーカーへ送信する
pub(super) fn submit_preview_request(
    tx: &mpsc::Sender<PreviewRequest>,
    image_files: &[PathBuf],
    index: usize,
    state: &mut ViewerState,
    diff_mode: crate::model::config::ImageDiffMode,
    force: bool,
) -> bool {
    if let Some((expected_index, _)) = state.expected_preview_generation()
        && expected_index == index
        && !force
    {
        return false;
    }

    if let Some(path) = image_files.get(index) {
        state.increment_preview_generation();
        let generation = state.preview_generation();
        state.set_expected_preview_generation(Some((index, generation)));
        if tx
            .send(PreviewRequest {
                index,
                generation,
                path: path.clone(),
                diff_mode,
                transport_mode: state.transport_mode(),
                image_filter_type: state.image_filter_type(),
                image_width_limit: state.image_width_limit(),
                image_height_limit: state.image_height_limit(),
                allow_rgba_decode: state.image_render_state.active_image_id().is_some() && !force,
            })
            .is_err()
        {
            state.set_expected_preview_generation(None);
            return false;
        }
        return true;
    }

    false
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
        while let Ok(mut request) = req_rx.recv() {
            while let Ok(next) = req_rx.try_recv() {
                request = next;
            }

            let payload = prepare_image_payload(
                &request.path,
                request.diff_mode,
                request.transport_mode,
                request.image_filter_type,
                request.image_width_limit,
                request.image_height_limit,
                request.allow_rgba_decode,
            )
            .map_err(|e| e.to_string());
            if resp_tx
                .send(PreviewResponse {
                    index: request.index,
                    generation: request.generation,
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
