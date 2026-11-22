//! Windows HEIF primary-image decoding in an isolated libheif worker.
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};
use image::{DynamicImage, RgbaImage};

const MAX_BYTES: u64 = 256 * 1024 * 1024;
const MAX_PIXELS: u64 = 64 * 1024 * 1024;
const HEADER_BYTES: u64 = 12;

fn decoder_path() -> AppResult<PathBuf> {
    if let Some(path) = std::env::var_os("CLIPSEEK_HEIF_DECODER") {
        return Ok(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe() {
        if let Some(parent) = executable.parent() {
            for directory in [Some(parent), parent.parent()].into_iter().flatten() {
                let candidate = directory.join("heif-decoder.exe");
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }
    Err(AppError::Msg(
        "HEIC/HEIF decoder is missing; prepare the Windows HEIF resources".into(),
    ))
}

pub(super) fn decode(src: &Path) -> AppResult<DynamicImage> {
    use std::os::windows::process::CommandExt;
    if std::fs::metadata(src)?.len() > MAX_BYTES {
        return Err(AppError::Msg("HEIC/HEIF file is too large".into()));
    }
    let temp = tempfile::Builder::new()
        .prefix("clipseek-heif-")
        .tempdir()?;
    // Keep extended-length paths at the worker IO boundary, including TEMP.
    let source = std::fs::canonicalize(src)?;
    let output = std::fs::canonicalize(temp.path())?.join("primary.rgba");
    let decoder = std::fs::canonicalize(decoder_path()?)?;
    let mut worker = Command::new(decoder)
        .arg(source)
        .arg(&output)
        .current_dir(temp.path())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppError::Msg(format!("HEIC/HEIF decoder failed to start: {error}")))?;
    let started = Instant::now();
    loop {
        let status = match worker.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let _ = worker.kill();
                let _ = worker.wait();
                return Err(error.into());
            }
        };
        if let Some(status) = status {
            if !status.success() {
                return Err(AppError::Msg(
                    "HEIC/HEIF primary image decode failed".into(),
                ));
            }
            break;
        }
        if started.elapsed() > Duration::from_secs(60)
            || std::fs::metadata(&output)
                .map(|file| file.len() > MAX_BYTES + HEADER_BYTES)
                .unwrap_or(false)
        {
            let _ = worker.kill();
            let _ = worker.wait();
            return Err(AppError::Msg(
                "HEIC/HEIF decode exceeded its safety limit".into(),
            ));
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let size = std::fs::metadata(&output)?.len();
    if !(HEADER_BYTES..=MAX_BYTES + HEADER_BYTES).contains(&size) {
        return Err(AppError::Msg(
            "HEIC/HEIF output exceeds the safety limit".into(),
        ));
    }
    let mut data = std::fs::read(output)?;
    if data.len() as u64 != size || data.get(..4) != Some(b"CSHF") {
        return Err(AppError::Msg("HEIC/HEIF output header is invalid".into()));
    }
    let width = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let height = u32::from_le_bytes(data[8..12].try_into().unwrap());
    let pixels = u64::from(width) * u64::from(height);
    if width == 0 || height == 0 || pixels > MAX_PIXELS || size != HEADER_BYTES + pixels * 4 {
        return Err(AppError::Msg(
            "HEIC/HEIF output dimensions are invalid".into(),
        ));
    }
    data.copy_within(12.., 0);
    data.truncate(data.len() - 12);
    let image = RgbaImage::from_raw(width, height, data)
        .ok_or_else(|| AppError::Msg("HEIC/HEIF pixel data is invalid".into()))?;
    Ok(DynamicImage::ImageRgba8(image))
}
