use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::{io, path::PathBuf};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 文字列を表示セル幅基準で切り詰める関数
fn truncate_to_display_width(text: &str, max_width: usize) -> String {
    if max_width == 0 {
        return String::new();
    }

    if text.width() <= max_width {
        return text.to_string();
    }

    let ellipsis = '…';
    let ellipsis_width = UnicodeWidthChar::width(ellipsis).unwrap_or(1);
    if max_width <= ellipsis_width {
        return ".".repeat(max_width);
    }

    let mut out = String::new();
    let mut used_width = 0;
    let target_width = max_width - ellipsis_width;
    for ch in text.chars() {
        let w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if used_width + w > target_width {
            break;
        }
        used_width += w;
        out.push(ch);
    }
    out.push(ellipsis);
    out
}

/// 画像のヘッダー部分を描画する関数
pub fn render_header(
    stdout: &mut io::Stdout,
    image_files: &[PathBuf],
    current_index: usize,
    term_width: u32,
    header_bg_color: Color,
    header_fg_color: Color,
) -> Result<()> {
    let total_count = image_files.len();

    let current_name = image_files
        .get(current_index)
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let prev_index = if total_count == 0 {
        0
    } else if current_index == 0 {
        total_count - 1
    } else {
        current_index - 1
    };
    let next_index = if total_count == 0 {
        0
    } else if current_index + 1 < total_count {
        current_index + 1
    } else {
        0
    };

    let prev_name = image_files
        .get(prev_index)
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let next_name = image_files
        .get(next_index)
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    let header = format!(
        " {} <- {} -> {} ({}/{})",
        prev_name,
        current_name,
        next_name,
        current_index + 1,
        total_count
    );

    // 端末幅を超えると次行に折り返されるため、表示セル幅基準で切り詰める。
    let width = term_width as usize;
    let mut line = truncate_to_display_width(&header, width);
    let remaining = width.saturating_sub(line.width());
    line.push_str(&" ".repeat(remaining));

    queue!(
        stdout,
        MoveTo(0, 0),
        SetBackgroundColor(header_bg_color),
        SetForegroundColor(header_fg_color),
        Print(line),
        ResetColor
    )?;

    Ok(())
}
