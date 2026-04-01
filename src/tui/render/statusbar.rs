use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
    terminal::{Clear, ClearType},
};
use std::{io, time::Duration};
use unicode_width::UnicodeWidthStr;

/// 画面下部にステータスバーを描画する関数
pub fn render_statusbar(
    stdout: &mut io::Stdout,
    term_width: u32,
    term_height: u32,
    elapsed: Duration,
    image_dimensions: (u32, u32),
    statusbar_bg_color: Color,
    statusbar_fg_color: Color,
) -> Result<()> {
    if term_height == 0 {
        return Ok(());
    }

    let elapsed_ms = elapsed.as_secs_f64() * 1000.0;
    let (img_width, img_height) = image_dimensions;
    let text = format!("{:.1}ms | {}x{}", elapsed_ms, img_width, img_height);

    // 最下行での自動折り返しスクロールを防ぐため、末尾1セルは使わない。
    let width = term_width.saturating_sub(1) as usize;
    let mut line = if text.width() > width {
        let mut out = String::new();
        let mut used_width = 0;
        for ch in text.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + w > width {
                break;
            }
            used_width += w;
            out.push(ch);
        }
        out
    } else {
        text
    };
    let remaining = width.saturating_sub(line.width());
    line.push_str(&" ".repeat(remaining));

    queue!(
        stdout,
        MoveTo(0, term_height.saturating_sub(1) as u16),
        Clear(ClearType::CurrentLine),
        SetBackgroundColor(statusbar_bg_color),
        SetForegroundColor(statusbar_fg_color),
        Print(line),
        ResetColor
    )?;

    Ok(())
}
