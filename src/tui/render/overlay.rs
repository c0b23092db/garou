use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::{io, path::Path};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

#[derive(Debug, Clone)]
pub struct OverlayInfo {
    pub image_dimensions: (u32, u32),
    pub file_size_bytes: Option<u64>,
    pub file_name: String,
    pub format: String,
}

pub fn render_overlay(
    stdout: &mut io::Stdout,
    term_width: u32,
    term_height: u32,
    info: &OverlayInfo,
) -> Result<()> {
    if term_width < 8 || term_height < 6 {
        return Ok(());
    }

    let size_text = format_file_size(info.file_size_bytes);
    let dims_text = format!("{}x{}", info.image_dimensions.0, info.image_dimensions.1);

    let mut lines = [
        format!("Name: {}", info.file_name),
        format!("Format: {}", info.format),
        format!("Dimensions: {}", dims_text),
        format!("Size: {}", size_text),
    ];

    let max_line_width = lines
        .iter()
        .map(|line| line.width())
        .max()
        .unwrap_or(0)
        .min(term_width.saturating_sub(4) as usize);

    let inner_width = max_line_width.max(20);
    let box_width = inner_width.saturating_add(2);
    let box_width_u16 = u16::try_from(box_width).unwrap_or(u16::MAX);
    let box_height = lines.len() as u16 + 2;

    let start_x = ((term_width as u16).saturating_sub(box_width_u16)) / 2;
    let start_y = ((term_height as u16).saturating_sub(box_height)) / 2;

    let horizontal = "-".repeat(inner_width);
    let top = format!("+{}+", horizontal);
    let bottom = top.clone();

    queue!(
        stdout,
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::White)
    )?;

    queue!(stdout, MoveTo(start_x, start_y), Print(top))?;

    for (idx, line) in lines.iter_mut().enumerate() {
        let y = start_y + 1 + idx as u16;
        let truncated = truncate_to_width(line, inner_width);
        let padding = inner_width.saturating_sub(truncated.width());
        let row = format!("|{}{}|", truncated, " ".repeat(padding));
        queue!(stdout, MoveTo(start_x, y), Print(row))?;
    }

    queue!(
        stdout,
        MoveTo(start_x, start_y + box_height - 1),
        Print(bottom),
        ResetColor
    )?;

    Ok(())
}

fn format_file_size(bytes: Option<u64>) -> String {
    let Some(bytes) = bytes else {
        return "unknown".to_string();
    };
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let kib = bytes as f64 / 1024.0;
    if kib < 1024.0 {
        return format!("{:.1} KiB", kib);
    }
    let mib = kib / 1024.0;
    format!("{:.1} MiB", mib)
}

fn truncate_to_width(text: &str, max_width: usize) -> String {
    if text.width() <= max_width {
        return text.to_string();
    }

    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w + 3 > max_width {
            break;
        }
        used += w;
        out.push(ch);
    }
    out.push_str("...");
    out
}

pub fn build_overlay_info(path: &Path, image_dimensions: (u32, u32)) -> OverlayInfo {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();
    let format = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_uppercase())
        .unwrap_or_else(|| "UNKNOWN".to_string());
    let file_size_bytes = std::fs::metadata(path).ok().map(|meta| meta.len());

    OverlayInfo {
        image_dimensions,
        file_size_bytes,
        file_name,
        format,
    }
}