use anyhow::Result;
use std::{
    sync::mpsc::Receiver,
    time::{Duration, Instant},
};

use super::super::image_pipeline::PreparedImagePayload;
use super::super::state::RedrawMode;
use super::worker::{PreviewResponse, submit_preview_request};
use super::{RenderModeFlags, Viewer};

#[derive(Default)]
pub(super) struct PendingPreviewState {
    /// 描画待ちの準備済み画像 (index, generation, payload)
    pub(super) payload: Option<(usize, u64, PreparedImagePayload)>,
    pub(super) force_refresh: bool,
    pub(super) started_at: Option<Instant>,
    pub(super) loading_rendered: bool,
}

impl Viewer<'_> {
    /// 現在の画像を描画する。準備済みならすぐ描き、なければワーカーへ依頼して待機画面を描く。
    /// `force` は準備済みキャッシュと依頼中の要求を無視して作り直し、届いたら再転送する。
    /// 描画または依頼をしたら true
    pub(super) fn request_preview(&mut self, force: bool, flags: RenderModeFlags) -> Result<bool> {
        let lookup_started = Instant::now();
        if !force
            && let Some(path) = self.image_files.get(self.current_index)
            && let Some(mut prepared) = self.state.cache.prepared.get(path)
        {
            // 準備済みならワーカーを待たずに描画する。待機中の要求は世代を進めて無効化する。
            prepared.prepare_duration = lookup_started.elapsed();
            let preview = &mut self.state.preview;
            preview.preview_generation = preview.preview_generation.saturating_add(1);
            preview.expected_preview_generation = None;
            self.pending_preview = PendingPreviewState::default();
            self.render_prepared(prepared, flags)?;
            return Ok(true);
        }

        if submit_preview_request(
            &self.preview_req_tx,
            &self.image_files,
            self.current_index,
            &mut self.state,
            force,
        ) {
            self.pending_preview.started_at = Some(Instant::now());
            self.pending_preview.loading_rendered = false;
            self.pending_preview.force_refresh = force;
            self.render_pending(None, flags)?;
            return Ok(true);
        }

        Ok(false)
    }

    pub(super) fn drain_preview_responses(
        &mut self,
        preview_resp_rx: &Receiver<PreviewResponse>,
    ) -> Result<()> {
        while let Ok(response) = preview_resp_rx.try_recv() {
            if response.prefetch {
                self.accept_prefetch_response(response);
                continue;
            }
            if self.state.preview.expected_preview_generation
                != Some((response.index, response.generation))
            {
                continue;
            }

            self.state.preview.expected_preview_generation = None;
            self.pending_preview.started_at = None;
            self.pending_preview.loading_rendered = false;
            if response.index != self.current_index {
                continue;
            }
            match response.payload {
                Ok(payload) => {
                    self.state
                        .cache
                        .prepared
                        .insert(response.path, payload.clone());
                    self.pending_preview.payload =
                        Some((response.index, response.generation, payload));
                    self.redraw_mode = RedrawMode::ImageReplace;
                }
                Err(error) => {
                    self.pending_preview.force_refresh = false;
                    self.render_pending(Some(&error), RenderModeFlags::NONE)?;
                    self.redraw_mode = RedrawMode::Idle;
                }
            }
        }

        Ok(())
    }

    /// 先読み結果を準備済みキャッシュへ入れる。いま待っている表示要求と同じ画像ならそのまま描画に回す。
    fn accept_prefetch_response(&mut self, response: PreviewResponse) {
        let cache = &mut self.state.cache;
        cache.prefetch_inflight.remove(&response.path);
        // 端末サイズ変更前に準備された結果は使わない
        let Ok(payload) = response.payload else {
            return;
        };
        if response.generation != cache.prepared_epoch {
            return;
        }
        cache
            .prepared
            .insert(response.path.clone(), payload.clone());

        if let Some((index, generation)) = self.state.preview.expected_preview_generation
            && index == self.current_index
            && self.image_files.get(index) == Some(&response.path)
        {
            self.state.preview.expected_preview_generation = None;
            self.pending_preview.started_at = None;
            self.pending_preview.loading_rendered = false;
            self.pending_preview.payload = Some((index, generation, payload));
            self.redraw_mode = RedrawMode::ImageReplace;
        }
    }

    pub(super) fn maybe_render_loading_indicator(&mut self) -> Result<()> {
        let pending = &self.pending_preview;
        if self.state.preview.expected_preview_generation.is_some()
            && pending.payload.is_none()
            && !pending.loading_rendered
            && pending
                .started_at
                .is_some_and(|started| started.elapsed() >= Duration::from_millis(100))
        {
            self.render_pending(Some("Load image access time"), RenderModeFlags::NONE)?;
            self.pending_preview.loading_rendered = true;
        }

        Ok(())
    }

    pub(super) fn handle_refresh_redraw(&mut self, flags: RenderModeFlags) -> Result<()> {
        if !self.request_preview(true, flags)? {
            self.render_current(flags)?;
        }
        Ok(())
    }

    pub(super) fn handle_image_replace_redraw(&mut self) -> Result<()> {
        if let Some((index, generation, payload)) = self.pending_preview.payload.take()
            && index == self.current_index
            && generation == self.state.preview.preview_generation
        {
            let flags = RenderModeFlags {
                refresh_image: self.pending_preview.force_refresh,
                full_refresh: false,
            };
            self.render_prepared(payload, flags)?;
        }

        self.pending_preview.force_refresh = false;
        Ok(())
    }
}
