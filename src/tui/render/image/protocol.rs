//! Kitty Graphics Protocol の送信シーケンスを組み立てる

use anyhow::Result;
use std::io::{self, Write};

use super::{
    layout::Placement,
    transport::{ResolvedTransport, UploadPayload, UploadPixelFormat},
};

const KITTY_IMAGE_ID: u32 = 1;

/// 1 回のエスケープシーケンスに載せる base64 の最大バイト数（プロトコルの規定値で、4 の倍数）
const CHUNK_SIZE: usize = 4096;

/// 画像の重ね順。文字とセル背景色より下に置く。
/// z=0 だと Ghostty / kitty ではオーバーレイなどの文字が画像に隠れる。
const IMAGE_Z_INDEX: i32 = -1_073_741_825;

/// 配置キー（表示セル数・切り出し範囲・重ね順）を組み立てる。
/// 配置ID (p=) は付けない: WezTerm では付けると画像の差し替えやテキストの重ね描きが反映されなくなる。
fn placement_keys(placement: &Placement) -> String {
    let mut keys = format!(
        "C=1,c={},r={},z={IMAGE_Z_INDEX}",
        placement.cols, placement.rows
    );
    if !placement.is_full_crop() {
        let (x, y, w, h) = placement.crop;
        keys.push_str(&format!(",x={x},y={y},w={w},h={h}"));
    }
    keys
}

/// 現在表示中の画像IDをターミナル側メモリから削除する
pub fn send_delete(stdout: &mut io::Stdout) -> Result<()> {
    write!(stdout, "\x1b_Ga=d,q=2,i={}\x1b\\", KITTY_IMAGE_ID)?;
    Ok(())
}

/// コマンドを 1 つのシーケンスで送る。
/// どのコマンドにも q=2 を付ける: 付けないと端末の応答 (ESC _ G i=1;OK ESC \) が
/// 標準入力に届き、キー入力として解釈されてしまう。
fn write_command(out: &mut impl Write, keys: &str, payload: &str) -> io::Result<()> {
    write!(out, "\x1b_G{keys},q=2;{payload}\x1b\\")
}

/// direct 転送の payload を分割して送る。2 個目以降のシーケンスには m と q だけを付ける
fn write_chunked(out: &mut impl Write, keys: &str, payload: &str) -> io::Result<()> {
    let mut chunks = payload.as_bytes().chunks(CHUNK_SIZE).peekable();
    let mut first = true;
    loop {
        let chunk = chunks.next().unwrap_or_default();
        let more = u8::from(chunks.peek().is_some());
        if first {
            write!(out, "\x1b_G{keys},q=2,m={more};")?;
        } else {
            write!(out, "\x1b_Gm={more},q=2;")?;
        }
        out.write_all(chunk)?;
        out.write_all(b"\x1b\\")?;
        first = false;
        if more == 0 {
            return Ok(());
        }
    }
}

/// payload を送信しつつ画像を表示する
pub fn send_upload(
    stdout: &mut io::Stdout,
    placement: Placement,
    upload_payload: &UploadPayload,
) -> Result<()> {
    let keys = placement_keys(&placement);
    let payload = upload_payload.payload.as_ref();

    match upload_payload.transport {
        ResolvedTransport::Direct => {
            let command = if upload_payload.pixel_format == UploadPixelFormat::Rgba {
                format!(
                    "a=T,f=32,t=d,s={},v={},{keys},i={KITTY_IMAGE_ID}",
                    upload_payload.pixel_width, upload_payload.pixel_height
                )
            } else {
                format!("a=T,f=100,t=d,{keys},i={KITTY_IMAGE_ID}")
            };
            write_chunked(stdout, &command, payload)?;
        }
        ResolvedTransport::File => {
            let command = format!("a=T,f=100,t=f,{keys},i={KITTY_IMAGE_ID}");
            write_command(stdout, &command, payload)?;
        }
        ResolvedTransport::TempFile => {
            let command = format!("a=T,f=100,t=t,{keys},i={KITTY_IMAGE_ID}");
            write_command(stdout, &command, payload)?;
        }
        ResolvedTransport::SharedMemory => {
            let command = format!(
                "a=T,f=100,t=s,s={},{keys},i={KITTY_IMAGE_ID}",
                upload_payload.data_size
            );
            write_command(stdout, &command, payload)?;
        }
    }

    Ok(())
}

/// 既存画像IDを指定セルサイズで再配置する。
/// 前の配置を消してから置くので、縮小時に古い配置がはみ出して残らない（画像データは残す: d=i）。
pub fn send_place(stdout: &mut io::Stdout, placement: Placement) -> Result<()> {
    write!(
        stdout,
        "\x1b_Ga=d,d=i,q=2,i={}\x1b\\\x1b_Ga=p,q=2,{},i={}\x1b\\",
        KITTY_IMAGE_ID,
        placement_keys(&placement),
        KITTY_IMAGE_ID
    )?;
    Ok(())
}

/// 既存画像へ RGBA 矩形パッチを重ねる。
/// r=1 で表示中のフレームを編集する（省略すると新しいアニメーションフレームが作られて表示が変わらない）。
/// X=1 で透明部分も含めて上書きする。
/// 分割しない: WezTerm は分割された a=f を 1 個目のデータだけで処理してエラーにする。
pub fn send_patch_rgba(
    stdout: &mut io::Stdout,
    offset_x: u32,
    offset_y: u32,
    patch_width: u32,
    patch_height: u32,
    encoded_payload: &str,
) -> Result<()> {
    let command = format!(
        "a=f,r=1,X=1,f=32,t=d,i={KITTY_IMAGE_ID},x={offset_x},y={offset_y},s={patch_width},v={patch_height}"
    );
    write_command(stdout, &command, encoded_payload)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunked(payload: &str) -> String {
        let mut out = Vec::new();
        write_chunked(&mut out, "a=T,i=1", payload).unwrap();
        String::from_utf8(out).unwrap()
    }

    #[test]
    fn short_payload_is_a_single_quiet_command() {
        assert_eq!(chunked("AAAA"), "\x1b_Ga=T,i=1,q=2,m=0;AAAA\x1b\\");
    }

    #[test]
    fn long_payload_is_split_at_the_protocol_limit() {
        let a = "A".repeat(CHUNK_SIZE);
        let expected = format!(
            "\x1b_Ga=T,i=1,q=2,m=1;{a}\x1b\\\x1b_Gm=1,q=2;{a}\x1b\\\x1b_Gm=0,q=2;BBBB\x1b\\"
        );
        assert_eq!(chunked(&format!("{a}{a}BBBB")), expected);
    }

    #[test]
    fn payload_of_exactly_one_chunk_does_not_send_an_empty_tail() {
        let a = "A".repeat(CHUNK_SIZE);
        assert_eq!(chunked(&a), format!("\x1b_Ga=T,i=1,q=2,m=0;{a}\x1b\\"));
    }
}
