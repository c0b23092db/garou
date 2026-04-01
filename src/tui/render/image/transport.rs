//! 転送モード設定を実行時の転送方式へ解決する

use crate::model::config::TransportMode;
use base64::{Engine as _, engine::general_purpose};
use std::env;
use std::{fs, path::PathBuf};

#[cfg(unix)]
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(unix)]
static SHM_COUNTER: AtomicU64 = AtomicU64::new(1);

/// 実際に利用する転送方式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedTransport {
    /// t=d: 直接転送する方式。ペイロードに base64 エンコードした画像データをそのまま渡す。
    Direct,
    /// t=f: ファイルパスを渡して端末側に読み込ませる方式。
    File,
    /// t=t: 一時ファイルパスを渡して端末側に読み込ませる方式。
    TempFile,
    /// t=s: 共有メモリ経由で転送する方式。転送前に shared memory に画像データを書き込み、ペイロードには共有メモリの識別子を base64 エンコードして渡す。
    SharedMemory,
}

/// プロトコル送信に必要な転送情報
#[derive(Debug, Clone)]
pub struct UploadPayload {
    /// 利用する転送方式
    pub transport: ResolvedTransport,
    /// 画像データを転送するためのペイロード
    pub payload: String,
    /// 画像データの生サイズ
    pub data_size: usize,
}

/// shared memory のライフサイクルを保持する状態
#[derive(Debug, Default)]
pub struct SharedMemoryState {
    #[cfg(unix)]
    segment: Option<PosixShmSegment>,
    file_path: Option<PathBuf>,
    temp_file_path: Option<PathBuf>,
}

/// 現在のセッションが SSH 経由かどうかを判定する
fn is_ssh_session() -> bool {
    env::var_os("SSH_CONNECTION").is_some()
        || env::var_os("SSH_CLIENT").is_some()
        || env::var_os("SSH_TTY").is_some()
}

/// shared memory 転送に対応している端末かを環境変数から判定する
fn is_shared_memory_terminal() -> bool {
    let term_program = env::var("TERM_PROGRAM")
        .unwrap_or_default()
        .to_ascii_lowercase();
    let term = env::var("TERM").unwrap_or_default().to_ascii_lowercase();

    // 既知の対応端末を優先判定する
    term_program.contains("wezterm")
        || term_program.contains("kitty")
        || term.contains("wezterm")
        || term.contains("xterm-kitty")
}

/// 現在環境で shared memory 転送を有効化できるかを判定する
fn can_enable_shared_memory() -> bool {
    // Kitty の shared memory は POSIX 前提。Windows は direct に固定する。
    cfg!(unix) && !is_ssh_session() && is_shared_memory_terminal()
}

/// 実行時の最終転送方式を解決する
pub fn resolve_transport_mode(mode: TransportMode) -> ResolvedTransport {
    match mode {
        TransportMode::Auto | TransportMode::SharedMemory => {
            if can_enable_shared_memory() {
                ResolvedTransport::SharedMemory
            } else {
                ResolvedTransport::Direct
            }
        }
        TransportMode::Direct => ResolvedTransport::Direct,
        TransportMode::File => ResolvedTransport::File,
        TransportMode::TempFile => ResolvedTransport::TempFile,
    }
}

/// 指定された転送方式でアップロード用の payload を準備する
pub fn prepare_upload_payload(
    requested: ResolvedTransport,
    encoded_payload: &str,
    image_data: &[u8],
    shm_state: &mut SharedMemoryState,
) -> UploadPayload {
    match requested {
        ResolvedTransport::File => {
            if let Some(payload) = shm_state.write_file_payload(image_data, false) {
                return payload;
            }

            UploadPayload {
                transport: ResolvedTransport::Direct,
                payload: encoded_payload.to_string(),
                data_size: image_data.len(),
            }
        }
        ResolvedTransport::TempFile => {
            if let Some(payload) = shm_state.write_file_payload(image_data, true) {
                return payload;
            }

            UploadPayload {
                transport: ResolvedTransport::Direct,
                payload: encoded_payload.to_string(),
                data_size: image_data.len(),
            }
        }
        ResolvedTransport::SharedMemory => {
            #[cfg(unix)]
            {
                if let Some(payload) = shm_state.write(image_data) {
                    return payload;
                }
            }

            UploadPayload {
                transport: ResolvedTransport::Direct,
                payload: encoded_payload.to_string(),
                data_size: image_data.len(),
            }
        }
        ResolvedTransport::Direct => UploadPayload {
            transport: ResolvedTransport::Direct,
            payload: encoded_payload.to_string(),
            data_size: image_data.len(),
        },
    }
}

impl SharedMemoryState {
    fn write_file_payload(&mut self, image_data: &[u8], temp_file: bool) -> Option<UploadPayload> {
        let target = if temp_file {
            let path = unique_temp_png_path();
            self.temp_file_path = Some(path.clone());
            path
        } else {
            let path = self
                .file_path
                .clone()
                .unwrap_or_else(|| unique_named_png_path("garou-kitty-file"));
            self.file_path = Some(path.clone());
            path
        };

        if fs::write(&target, image_data).is_err() {
            return None;
        }

        let path_str = target.to_string_lossy();
        let payload = general_purpose::STANDARD.encode(path_str.as_bytes());

        Some(UploadPayload {
            transport: if temp_file {
                ResolvedTransport::TempFile
            } else {
                ResolvedTransport::File
            },
            payload,
            data_size: image_data.len(),
        })
    }
}

fn unique_named_png_path(prefix: &str) -> PathBuf {
    let mut path = env::temp_dir();
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    path.push(format!("{}-{}-{}.png", prefix, pid, nanos));
    path
}

fn unique_temp_png_path() -> PathBuf {
    unique_named_png_path("garou-kitty-temp")
}

#[cfg(unix)]
impl SharedMemoryState {
    fn write(&mut self, image_data: &[u8]) -> Option<UploadPayload> {
        let segment = PosixShmSegment::create(image_data).ok()?;
        let payload = general_purpose::STANDARD.encode(segment.name().as_bytes());
        self.segment = Some(segment);

        Some(UploadPayload {
            transport: ResolvedTransport::SharedMemory,
            payload,
            data_size: image_data.len(),
        })
    }
}

#[cfg(unix)]
#[derive(Debug)]
struct PosixShmSegment {
    name: String,
}

#[cfg(unix)]
impl PosixShmSegment {
    fn name(&self) -> &str {
        &self.name
    }

    fn create(data: &[u8]) -> std::io::Result<Self> {
        use std::{ffi::CString, io, ptr};

        if data.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shared memory payload is empty",
            ));
        }

        let pid = std::process::id();
        let seq = SHM_COUNTER.fetch_add(1, Ordering::Relaxed);
        let name = format!("/garou-{}-{}", pid, seq);
        let c_name = CString::new(name.clone())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid shm name"))?;

        // SAFETY: FFI 呼び出しは POSIX API の契約に従って行う
        unsafe {
            let fd = libc::shm_open(
                c_name.as_ptr(),
                libc::O_CREAT | libc::O_EXCL | libc::O_RDWR,
                0o600,
            );
            if fd < 0 {
                return Err(io::Error::last_os_error());
            }

            let cleanup_and_err = |fd: i32| {
                let err = io::Error::last_os_error();
                let _ = libc::close(fd);
                let _ = libc::shm_unlink(c_name.as_ptr());
                err
            };

            if libc::ftruncate(fd, data.len() as libc::off_t) != 0 {
                return Err(cleanup_and_err(fd));
            }

            let mapped = libc::mmap(
                ptr::null_mut(),
                data.len(),
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if mapped == libc::MAP_FAILED {
                return Err(cleanup_and_err(fd));
            }

            ptr::copy_nonoverlapping(data.as_ptr(), mapped as *mut u8, data.len());

            if libc::munmap(mapped, data.len()) != 0 {
                let err = cleanup_and_err(fd);
                return Err(err);
            }

            if libc::close(fd) != 0 {
                let _ = libc::shm_unlink(c_name.as_ptr());
                return Err(io::Error::last_os_error());
            }
        }

        Ok(Self { name })
    }
}

#[cfg(unix)]
impl Drop for PosixShmSegment {
    fn drop(&mut self) {
        if let Ok(c_name) = std::ffi::CString::new(self.name.clone()) {
            // SAFETY: CString は NUL 終端済みで POSIX API に渡せる
            unsafe {
                let _ = libc::shm_unlink(c_name.as_ptr());
            }
        }
    }
}
