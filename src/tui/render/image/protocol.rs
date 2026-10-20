//! Kitty Graphics Protocol の送信シーケンスを組み立てる

use anyhow::Result;
use std::io::{self, Write};

use super::{
    layout::Placement,
    transport::{ResolvedTransport, UploadPayload, UploadPixelFormat},
};

const KITTY_IMAGE_ID: u32 = 1;

/// 配置キー（表示セル数・切り出し範囲）を組み立てる。
/// 配置ID (p=) は付けない: WezTerm では付けると画像の差し替えやテキストの重ね描きが反映されなくなる。
fn placement_keys(placement: &Placement) -> String {
    let mut keys = format!("C=1,c={},r={}", placement.cols, placement.rows);
    if !placement.is_full_crop() {
        let (x, y, w, h) = placement.crop;
        keys.push_str(&format!(",x={x},y={y},w={w},h={h}"));
    }
    keys
}

/// 現在表示中の画像IDをターミナル側メモリから削除する
pub fn send_delete(stdout: &mut io::Stdout) -> Result<()> {
    write!(stdout, "\x1b_Ga=d,i={}\x1b\\", KITTY_IMAGE_ID)?;
    Ok(())
}

/// payload を送信しつつ画像を表示する
pub fn send_upload(
    stdout: &mut io::Stdout,
    placement: Placement,
    upload_payload: &UploadPayload,
) -> Result<()> {
    let keys = placement_keys(&placement);

    match upload_payload.transport {
        ResolvedTransport::Direct => {
            if upload_payload.pixel_format == UploadPixelFormat::Rgba {
                write!(
                    stdout,
                    "\x1b_Ga=T,f=32,t=d,s={},v={},{},i={};{}\x1b\\",
                    upload_payload.pixel_width,
                    upload_payload.pixel_height,
                    keys,
                    KITTY_IMAGE_ID,
                    upload_payload.payload
                )?;
            } else {
                write!(
                    stdout,
                    "\x1b_Ga=T,f=100,t=d,{},i={};{}\x1b\\",
                    keys, KITTY_IMAGE_ID, upload_payload.payload
                )?;
            }
        }
        ResolvedTransport::File => {
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=f,{},i={};{}\x1b\\",
                keys, KITTY_IMAGE_ID, upload_payload.payload
            )?;
        }
        ResolvedTransport::TempFile => {
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=t,{},i={};{}\x1b\\",
                keys, KITTY_IMAGE_ID, upload_payload.payload
            )?;
        }
        ResolvedTransport::SharedMemory => {
            write!(
                stdout,
                "\x1b_Ga=T,f=100,t=s,s={},{},i={};{}\x1b\\",
                upload_payload.data_size, keys, KITTY_IMAGE_ID, upload_payload.payload
            )?;
        }
    }

    Ok(())
}

/// 既存画像IDを指定セルサイズで再配置する。
/// 前の配置を消してから置くので、縮小時に古い配置がはみ出して残らない（画像データは残す: d=i）。
pub fn send_place(stdout: &mut io::Stdout, placement: Placement) -> Result<()> {
    write!(
        stdout,
        "\x1b_Ga=d,d=i,i={}\x1b\\\x1b_Ga=p,{},i={}\x1b\\",
        KITTY_IMAGE_ID,
        placement_keys(&placement),
        KITTY_IMAGE_ID
    )?;
    Ok(())
}

/// 既存画像へ RGBA 矩形パッチを重ねる
pub fn send_patch_rgba(
    stdout: &mut io::Stdout,
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
