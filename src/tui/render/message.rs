use anyhow::Result;
use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};
use std::io;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 画像領域の中央にメッセージを表示
pub fn render_image_message_center(
    stdout: &mut io::Stdout,
    image_start_x: u16,
    available_width: u32,
    available_height: u32,
    message: &str,
) -> Result<()> {
    if available_width == 0 || available_height == 0 {
        return Ok(());
    }

    let max_width = available_width.saturating_sub(2).max(1) as usize;
    let mut line = if message.width() > max_width {
        let mut out = String::new();
        let mut used_width = 0;
        for ch in message.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + ch_width > max_width {
                break;
            }
            used_width += ch_width;
            out.push(ch);
        }
        out
    } else {
        message.to_string()
    };

    let padding = max_width.saturating_sub(line.width());
    line.push_str(&" ".repeat(padding));

    let line_width = line.width() as u16;
    let centered_x =
        image_start_x.saturating_add((available_width as u16).saturating_sub(line_width) / 2);
    let centered_y = 1u16.saturating_add((available_height as u16) / 2);

    queue!(
        stdout,
        MoveTo(centered_x, centered_y),
        SetBackgroundColor(Color::DarkGrey),
        SetForegroundColor(Color::White),
        Print(line),
        ResetColor
    )?;

    Ok(())
}

/// 画像領域の左上にメッセージを表示
pub fn render_image_message_top_left(
    stdout: &mut io::Stdout,
    image_start_x: u16,
    available_width: u32,
    message: &str,
) -> Result<()> {
    if available_width == 0 {
        return Ok(());
    }

    let max_width = available_width.saturating_sub(2).max(1) as usize;
    let mut line = if message.width() > max_width {
        let mut out = String::new();
        let mut used_width = 0;
        for ch in message.chars() {
            let ch_width = UnicodeWidthChar::width(ch).unwrap_or(0);
            if used_width + ch_width > max_width {
                break;
            }
            used_width += ch_width;
            out.push(ch);
        }
        out
    } else {
        message.to_string()
    };

    let padding = max_width.saturating_sub(line.width());
    line.push_str(&" ".repeat(padding));

    queue!(
        stdout,
        MoveTo(image_start_x, 1),
        SetForegroundColor(Color::White),
        Print(line),
        ResetColor
    )?;

    Ok(())
}
