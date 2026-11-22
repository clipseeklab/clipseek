//! macOS HEIC/HEIF decoding uses /usr/bin/sips because generic MOV decoding may return only
//! one tile of a grid image. Convert to temporary JPEG with orientation applied, then reuse
//! the image pipeline.

use std::path::Path;
use std::process::Command;

use crate::error::{AppError, AppResult};

const SIPS: &str = "/usr/bin/sips";

/// Check sips availability; unsupported outside macOS.
pub fn available() -> bool {
    std::path::Path::new(SIPS).exists()
}

/// Convert HEIC/HEIF to JPEG with orientation applied. The caller chooses and later removes
/// the temporary destination.
pub fn heic_to_jpeg(src: &Path, dest: &Path) -> AppResult<()> {
    let out = Command::new(SIPS)
        .args(["-s", "format", "jpeg"])
        .arg(src)
        .arg("--out")
        .arg(dest)
        .output()
        .map_err(|e| AppError::Msg(format!("sips spawn failed: {e}")))?;
    if !out.status.success() || !dest.exists() {
        let msg = String::from_utf8_lossy(&out.stderr);
        return Err(AppError::Msg(format!("sips HEIC transcode failed: {msg}")));
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/media_sips.rs"]
mod tests;
