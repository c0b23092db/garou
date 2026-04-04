use anyhow::Result;
use std::{path::PathBuf, sync::mpsc, thread};

use super::super::image_pipeline::{PreparedImagePayload, prepare_image_payload};
use super::super::state::ViewerState;
use crate::model::config::TransportMode;

#[derive(Debug, Clone)]
pub(super) struct PreviewRequest {
    pub index: usize,
    pub generation: u64,
    pub path: PathBuf,
    pub diff_mode: crate::model::config::ImageDiffMode,
    pub transport_mode: TransportMode,
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

            let payload =
                prepare_image_payload(&request.path, request.diff_mode, request.transport_mode)
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
