use std::{sync::Arc, time::Duration};

use anyhow::Result;

use super::super::image_pipeline::{
    PreparedImagePayload, is_always_upload_mode, prepare_image_payload,
};
use super::super::render::{FrameRenderInput, FrameRenderMetrics, RenderOptions, render_frame};
use super::{RenderModeFlags, Viewer};

impl Viewer<'_> {
    /// 状態から決まる描画オプション。画像の項目は待機画面用の空値
    fn base_options(&self, flags: RenderModeFlags) -> RenderOptions {
        let ui = &self.state.ui_state;
        let image = &self.state.image_config;
        RenderOptions {
            refresh_image: flags.refresh_image,
            full_refresh: flags.full_refresh,
            skip_image: true,
            preserve_image: true,
            sidebar_visible: ui.sidebar_visible,
            header_visible: ui.header_visible,
            statusbar_visible: ui.statusbar_visible,
            sidebar_size: ui.sidebar_size,
            header_bg_color: ui.header_bg_color,
            header_fg_color: ui.header_fg_color,
            statusbar_bg_color: ui.statusbar_bg_color,
            statusbar_fg_color: ui.statusbar_fg_color,
            always_upload: false,
            transport_mode: image.transport_mode,
            diff_mode: image.image_diff_mode,
            image_dimensions: (0, 0),
            source_dimensions: (0, 0),
            payload_hash: 0,
            image_data: Arc::<[u8]>::from([]),
            encoded_payload: Arc::<str>::from(""),
            prepared_upload_payload: None,
            dirty_ratio: image.dirty_ratio,
            tile_grid: image.tile_grid,
            skip_step: image.skip_step,
            zoom_factor: image.zoom_factor,
            pan_center: image.pan_center,
            rgba_frame: None,
            overlay_visible: ui.overlay_visible,
            status_message: None,
            processing_duration: Duration::ZERO,
        }
    }

    /// 画面全体を描画する
    fn draw(&mut self, options: RenderOptions) -> Result<FrameRenderMetrics> {
        let sidebar_entries = self
            .state
            .sidebar_tree
            .render_entries(self.image_files.get(self.current_index));
        render_frame(
            self.stdout,
            FrameRenderInput {
                image_files: &self.image_files,
                current_index: self.current_index,
                sidebar_entries: &sidebar_entries,
                term_width: self.viewport.width,
                term_height: self.viewport.height,
            },
            options,
            &mut self.state.image_render_state,
        )
    }

    /// 画像を描画して統計を記録する
    fn draw_image(&mut self, options: RenderOptions) -> Result<()> {
        let metrics = self.draw(options)?;
        let perf = &mut self.state.perf;
        perf.last_render_duration = metrics.render_duration;
        perf.last_dirty_tiles = metrics.dirty_tiles;
        perf.last_image_rect = Some(metrics.placement);
        Ok(())
    }

    /// 現在の画像をこのスレッドで準備して描画する（ワーカーに頼めないときの予備経路）
    pub(super) fn render_current(&mut self, flags: RenderModeFlags) -> Result<()> {
        let config = &self.state.image_config;
        let prepared = prepare_image_payload(
            &self.image_files[self.current_index],
            config.image_diff_mode,
            config.transport_mode,
            config.image_filter_type,
            config.image_width,
            config.image_height,
            self.state.image_render_state.has_image() && !flags.refresh_image,
            config.render_scale,
        )?;
        self.render_prepared(prepared, flags)
    }

    /// 準備済みの画像データを使って描画する
    pub(super) fn render_prepared(
        &mut self,
        prepared: PreparedImagePayload,
        flags: RenderModeFlags,
    ) -> Result<()> {
        let options = RenderOptions {
            skip_image: false,
            preserve_image: false,
            always_upload: is_always_upload_mode(self.state.image_config.image_diff_mode),
            transport_mode: prepared.transport_mode,
            image_dimensions: prepared.image_dimensions,
            source_dimensions: prepared.source_dimensions,
            payload_hash: prepared.payload_hash,
            image_data: prepared.image_data,
            encoded_payload: prepared.encoded_payload,
            prepared_upload_payload: prepared.prepared_upload_payload,
            rgba_frame: prepared.rgba_frame,
            processing_duration: prepared.prepare_duration,
            ..self.base_options(flags)
        };
        self.draw_image(options)
    }

    /// 非同期準備中の待機画面を描画する（表示中の画像は残す）
    pub(super) fn render_pending(
        &mut self,
        message: Option<&str>,
        flags: RenderModeFlags,
    ) -> Result<()> {
        let options = RenderOptions {
            status_message: message.map(str::to_owned),
            ..self.base_options(flags)
        };
        self.draw(options)?;
        Ok(())
    }
}
