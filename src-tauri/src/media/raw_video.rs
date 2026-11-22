//! ProRes RAW playback proxies.
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

// Versioned, relative names make proxies travel with the asset on library moves.
const PROXY: &str = "playback-720p-v1.mp4";
const MANIFEST: &str = "playback-720p-v1.json";
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize)]
struct Fingerprint {
    source_bytes: u64,
    modified_ns: u128,
}
fn fingerprint(source: &Path) -> AppResult<Fingerprint> {
    let meta = std::fs::metadata(source)?;
    Ok(Fingerprint {
        source_bytes: meta.len(),
        modified_ns: meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_err(|e| AppError::Msg(e.to_string()))?
            .as_nanos(),
    })
}
pub fn cached_proxy(source: &Path, directory: &Path) -> AppResult<Option<PathBuf>> {
    let proxy = directory.join(PROXY);
    let cached = std::fs::read(directory.join(MANIFEST))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Fingerprint>(&bytes).ok());
    let Some(cached) = cached else {
        return Ok(None);
    };
    if !proxy.metadata().is_ok_and(|m| m.is_file() && m.len() > 0) {
        return Ok(None);
    }
    Ok(if cached == fingerprint(source)? {
        Some(proxy)
    } else {
        None
    })
}
/// Transfer a valid derivative alongside a copied/reference asset. Rebind the
/// fingerprint because copying an original can change its filesystem mtime.
pub fn copy_cached_proxy(
    source: &Path,
    source_directory: &Path,
    target: &Path,
    target_directory: &Path,
) -> AppResult<()> {
    let Some(proxy) = cached_proxy(source, source_directory)? else {
        return Ok(());
    };
    let before = fingerprint(source)?;
    if before.source_bytes != std::fs::metadata(target)?.len() {
        return Ok(());
    }
    std::fs::create_dir_all(target_directory)?;
    let scratch = tempfile::Builder::new()
        .prefix(".playback-proxy-")
        .tempdir_in(target_directory)?;
    let copy = scratch.path().join(PROXY);
    std::fs::copy(proxy, &copy)?;
    if fingerprint(source)? != before {
        return Ok(());
    }
    let manifest = scratch.path().join(MANIFEST);
    std::fs::write(
        &manifest,
        serde_json::to_vec(&fingerprint(target)?).map_err(|e| AppError::Msg(e.to_string()))?,
    )?;
    let _ = std::fs::remove_file(target_directory.join(MANIFEST));
    std::fs::rename(copy, target_directory.join(PROXY))?;
    std::fs::rename(manifest, target_directory.join(MANIFEST))?;
    Ok(())
}

/// Includes the native intermediate, compact proxy, and a 64 MiB reserve.
pub fn required_space(duration_ms: i64) -> u64 {
    (duration_ms.max(0) as u64)
        .saturating_mul(5_000)
        .saturating_add(64 * 1024 * 1024)
}
pub fn is_prores_raw(source: &Path) -> bool {
    super::ffmpeg::video_codec(source).as_deref() == Some("prores_raw")
}

/// Keep Windows RAW decoding on the build validated with the camera fixtures.
#[cfg(target_os = "windows")]
pub fn ensure_raw_decoder() -> AppResult<()> {
    let output = super::ffmpeg_command()
        .args(["-hide_banner", "-decoders"])
        .output()?;
    if output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some("prores_raw"))
    {
        Ok(())
    } else {
        Err(AppError::coded(
            "raw_decoder_version",
            "This runtime does not decode ProRes RAW",
        ))
    }
}

#[cfg(target_os = "macos")]
fn native_export(source: &Path, target: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let helper_dir = tempfile::tempdir()?;
    let helper = helper_dir.path().join("native-video");
    std::fs::write(&helper, include_bytes!(env!("CLIPSEEK_NATIVE_VIDEO")))?;
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700))?;
    let output = std::process::Command::new(helper)
        .arg("proxy")
        .arg(source)
        .arg(target)
        .output()?;
    if !output.status.success() {
        return Err(AppError::coded(
            "playback_compatibility_failed",
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    if !target.metadata().is_ok_and(|m| m.len() > 0) {
        return Err(AppError::coded(
            "playback_compatibility_failed",
            "Native export produced an empty file",
        ));
    }
    Ok(())
}

/// All preview surfaces resolve RAW to a cached/generated proxy, never the original.
pub fn preview_source(source: &Path, directory: &Path) -> AppResult<PathBuf> {
    if is_prores_raw(source) {
        create_proxy(source, directory)
    } else {
        Ok(source.to_path_buf())
    }
}

pub fn create_proxy(source: &Path, directory: &Path) -> AppResult<PathBuf> {
    if !cfg!(any(target_os = "macos", target_os = "windows")) || !is_prores_raw(source) {
        return Err(AppError::coded(
            "playback_compatibility_failed",
            "Proxy conversion requires ProRes RAW on macOS or Windows",
        ));
    }
    if let Some(proxy) = cached_proxy(source, directory)? {
        return Ok(proxy);
    }
    // Automatic previews can originate from many hovered cards. Bound conversion
    // work across assets as well as serializing the same asset across processes.
    static CONVERSION: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _conversion = CONVERSION
        .lock()
        .map_err(|e| AppError::Msg(e.to_string()))?;
    std::fs::create_dir_all(directory)?;
    // Serialize conversions per asset across client instances. Source/reference
    // files are never used as lock files and are never changed.
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join(".playback-proxy.lock"))?;
    fs2::FileExt::lock_exclusive(&lock)?;
    if let Some(proxy) = cached_proxy(source, directory)? {
        return Ok(proxy);
    }
    let before = fingerprint(source)?;
    let info = super::ffmpeg::probe(source)?;
    if fs2::available_space(directory)? < required_space(info.duration_ms.unwrap_or(0)) {
        return Err(AppError::coded(
            "playback_compatibility_insufficient_space",
            "Not enough space for the proxy",
        ));
    }
    let scratch = tempfile::Builder::new()
        .prefix(".playback-proxy-")
        .tempdir_in(directory)?;
    let compact = scratch.path().join("compact.mp4");
    #[cfg(target_os = "macos")]
    {
        let native = scratch.path().join("native.mp4");
        native_export(source, &native)
            .map_err(|error| AppError::coded("playback_compatibility_failed", error.to_string()))?;
        // Native 720p presets can exceed 720 in one dimension and choose large
        // bitrates. This second hardware pass caps both dimensions and storage.
        let output = super::ffmpeg_command().args(["-v", "error", "-nostdin", "-y",
            "-hwaccel", "videotoolbox", "-i"]).arg(&native).args([
            "-map", "0:v:0", "-map", "0:a?", "-vf",
            "scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
            "-c:v", "h264_videotoolbox", "-allow_sw", "0", "-b:v", "1200k",
            "-maxrate", "1800k", "-bufsize", "2400k", "-pix_fmt", "yuv420p",
            "-c:a", "aac", "-b:a", "96k", "-ac", "2", "-fps_mode", "passthrough",
            "-movflags", "+faststart"]).arg(&compact).output()?;
        if !output.status.success() {
            return Err(AppError::coded(
                "playback_compatibility_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
    }
    #[cfg(target_os = "windows")]
    {
        ensure_raw_decoder()?;
        let output = super::ffmpeg_command()
            .args(["-v", "error", "-nostdin", "-y", "-xerror", "-i"])
            .arg(source)
            .args([
                "-map", "0:v:0", "-map", "0:a?", "-vf",
                "scale=w='min(1280,iw)':h='min(720,ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
                "-c:v", "libx264", "-preset", "veryfast", "-threads", "2",
                "-b:v", "1200k", "-maxrate", "1800k", "-bufsize", "2400k",
                "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "96k", "-ac", "2",
                "-fps_mode", "passthrough", "-movflags", "+faststart", "-abort_on", "empty_output",
            ]).arg(&compact).output()?;
        if !output.status.success() {
            return Err(AppError::coded(
                "playback_compatibility_failed",
                String::from_utf8_lossy(&output.stderr).trim().to_string(),
            ));
        }
        // Validate the complete derivative before its manifest makes it visible.
        let output = super::ffmpeg_command()
            .args(["-v", "error", "-nostdin", "-xerror", "-i"])
            .arg(&compact)
            .args(["-map", "0:v:0", "-map", "0:a?", "-f", "null", "-"])
            .output()?;
        if !output.status.success() || !compact.metadata().is_ok_and(|m| m.len() > 0) {
            return Err(AppError::coded(
                "playback_compatibility_failed",
                "Invalid playback proxy",
            ));
        }
    }
    if fingerprint(source)? != before {
        return Err(AppError::coded(
            "playback_compatibility_failed",
            "Source changed during proxy conversion; retry",
        ));
    }
    let manifest = scratch.path().join("manifest.json");
    std::fs::write(
        &manifest,
        serde_json::to_vec(&before).map_err(|e| AppError::Msg(e.to_string()))?,
    )?;
    // Drop stale validity before replacing media, so interruption cannot expose
    // an old manifest with a new proxy. Both files live on the destination volume.
    let _ = std::fs::remove_file(directory.join(MANIFEST));
    std::fs::rename(compact, directory.join(PROXY))?;
    std::fs::rename(manifest, directory.join(MANIFEST))?;
    Ok(directory.join(PROXY))
}
