use anyhow::Result;
use crossterm::event;
use std::time::{Duration, Instant};

use super::{RedrawMode, RenderModeFlags, Viewer};

pub(super) enum LoopStep {
    NotHandled,
    Continue,
    Break,
}

impl Viewer<'_> {
    /// デバウンス中の画像切り替えを期限が来たら実行する。待機中もイベントは受け付ける
    pub(super) fn pending_replace_phase(&mut self) -> Result<LoopStep> {
        let Some(deadline) = self.state.pending_deadline else {
            return Ok(LoopStep::NotHandled);
        };

        let now = Instant::now();
        // 準備済みの画像はデコード負荷がないのでデバウンスを待たずに描画する
        let prepared = self
            .image_files
            .get(self.current_index)
            .is_some_and(|path| self.state.cache.prepared.contains(path));
        if now >= deadline || prepared {
            self.state.pending_deadline = None;
            self.request_preview(false, RenderModeFlags::NONE)?;
            self.redraw_mode = RedrawMode::Idle;
            return Ok(LoopStep::Continue);
        }

        if event::poll(Duration::ZERO)? && self.handle_event(event::read()?).0 {
            return Ok(LoopStep::Break);
        }

        let nap = deadline
            .saturating_duration_since(now)
            .min(Duration::from_millis(5));
        std::thread::sleep(nap);
        Ok(LoopStep::Continue)
    }

    pub(super) fn polled_event_phase(&mut self, idle_poll_interval: Duration) -> Result<LoopStep> {
        if !event::poll(idle_poll_interval)? {
            return Ok(LoopStep::NotHandled);
        }

        let (should_quit, index_changed) = self.handle_event(event::read()?);

        if index_changed {
            if self.debounce_duration.is_zero() {
                self.request_preview(false, RenderModeFlags::NONE)?;
            }
            self.state.preview.last_idle_prefetch_at = None;
        }

        Ok(if should_quit {
            LoopStep::Break
        } else {
            LoopStep::Continue
        })
    }

    pub(super) fn maybe_run_idle_prefetch(&mut self, idle_prefetch_interval: Duration) {
        let state = &mut self.state;
        if self.redraw_mode != RedrawMode::Idle
            || state.pending_deadline.is_some()
            || state.preview.expected_preview_generation.is_some()
            || state.preview.prefetch_size == 0
        {
            return;
        }

        let now = Instant::now();
        if state
            .preview
            .last_idle_prefetch_at
            .is_some_and(|last| now.duration_since(last) < idle_prefetch_interval)
        {
            return;
        }

        super::worker::submit_prefetch_requests(
            &self.preview_req_tx,
            &self.image_files,
            self.current_index,
            state,
        );
        state.preview.last_idle_prefetch_at = Some(now);
    }
}
