mod core;
mod model;
mod tui;

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;
use tui::ConfigOption;

#[derive(Parser, Debug)]
#[command(version, about, arg_required_else_help = false)]
pub struct Args {
    /// Open Image file or Directory [defaults: current directory]
    #[arg(value_name = "PATH")]
    pub path: Option<PathBuf>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let config = model::config::AppConfig::load()?;

    let (image_files, start_index) =
        core::resolve_image_start(args.path, &config.image.extensions)?;
    if image_files.is_empty() {
        eprintln!("画像ファイルが見つかりません");
        return Ok(());
    }

    let mut stdout = std::io::stdout();
    tui::run_viewer(
        &mut stdout,
        &image_files,
        start_index,
        ConfigOption {
            sidebar_visible: config.display.sidebar,
            header_visible: config.display.header,
            statusbar_visible: config.display.statusbar,
            sidebar_size: config.display.sidebar_size,
            preview_debounce: config.display.preview_debounce,
            poll_interval: config.display.poll_interval,
            prefetch_interval: config.display.prefetch_interval,
            header_bg_color: config.display.header_bg_color,
            header_fg_color: config.display.header_fg_color,
            statusbar_bg_color: config.display.statusbar_bg_color,
            statusbar_fg_color: config.display.statusbar_fg_color,
            cache_lru_size: config.cache.lru_size,
            cache_max_bytes: config.cache.max_bytes,
            prefetch_size: config.cache.prefetch_size,
            image_diff_mode: config.image.diff_mode,
            transport_mode: config.image.transport_mode,
            dirty_ratio: config.image.dirty_ratio,
            tile_grid: config.image.tile_grid,
            skip_step: config.image.skip_step,
            image_width: config.image.image_width,
            image_height: config.image.image_height,
            image_filter_type: config.image.filter_type,
            image_extensions: config.image.extensions,
        },
    )?;

    Ok(())
}
