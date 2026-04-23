//! Kitty Graphics Protocol の送信シーケンスを組み立てる

use anyhow::Result;
use std::io::{self, Write};

use super::{
    layout::Placement,
    transport::{ResolvedTransport, UploadPayload, UploadPixelFormat},
};

const KITTY_IMAGE_ID: u32 = 1;

/// 現在表示中の画像IDをターミナル側メモリから削除する
pub fn send_delete(stdout: &mut io::Stdout, _image_id: u32) -> Result<()> {
    write!(stdout, "\x1b_Ga=d,i={}\x1b\\", KITTY_IMAGE_ID)?;
    Ok(())
}

/// payload を送信しつつ画像を表示する
pub fn send_upload(
    stdout: &mut io::Stdout,
    placement: Placement,
    _image_id: u32,
    upload_payload: &UploadPayload,
) -> Result<()> {
    let (_, _, display_width_cells, display_height_cells) = placement;

    match upload_payload.transport {
        ResolvedTransport::Direct => {
            if upload_payload.pixel_format == UploadPixelFormat::Rgba {
                write!(
                    stdout,
                    "\x1b_Ga=T,f=32,t=d,s={},v={},C=1,c={},r={},i={};{}\x1b\\",
                    upload_payload.pixel_width,
                    upload_payload.pixel_height,
                    display_width_cells,
                    display_height_cells,
                    KITTY_IMAGE_ID,
                    upload_payload.payload
                )?;
            } else {
                write!(
                    stdout,
                    "\x1b_Ga=T,f=100,t=d,C=1,c={},r={},i={};{}\x1b\\",
                    display_width_cells,
                    display_height_cells,
                    KITTY_IMAGE_ID,
                    upload_payload.payload
                )?;
            }
        }
        ResolvedTransport::File => {
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=f,C=1,c={},r={},i={};{}\x1b\\",
                display_width_cells, display_height_cells, KITTY_IMAGE_ID, upload_payload.payload
            )?;
        }
        ResolvedTransport::TempFile => {
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=t,C=1,c={},r={},i={};{}\x1b\\",
                display_width_cells, display_height_cells, KITTY_IMAGE_ID, upload_payload.payload
            )?;
        }
        ResolvedTransport::SharedMemory => {
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=s,s={},C=1,c={},r={},i={};{}\x1b\\",
                upload_payload.data_size,
                display_width_cells,
                display_height_cells,
                KITTY_IMAGE_ID,
                upload_payload.payload
            )?;
        }
    }

    Ok(())
}

/// 既存画像IDを指定セルサイズで再配置する
pub fn send_place(stdout: &mut io::Stdout, placement: Placement, _image_id: u32) -> Result<()> {
    let (_, _, display_width_cells, display_height_cells) = placement;
    write!(
        stdout,
        "\x1b_Ga=p,C=1,c={},r={},i={}\x1b\\",
        display_width_cells, display_height_cells, KITTY_IMAGE_ID
    )?;
    Ok(())
}

/// 既存画像へ RGBA 矩形パッチを重ねる
pub fn send_patch_rgba(
    stdout: &mut io::Stdout,
    _image_id: u32,
    offset_x: u32,
    offset_y: u32,
    patch_width: u32,
    patch_height: u32,
    encoded_payload: &str,
) -> Result<()> {
    write!(
        stdout,
        "\x1b_Ga=f,f=32,i={},x={},y={},s={},v={},C=1;{}\x1b\\",
        KITTY_IMAGE_ID, offset_x, offset_y, patch_width, patch_height, encoded_payload
    )?;
    Ok(())
}
