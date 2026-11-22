//! FFmpeg wrappers probe metadata and extract RGB24 frames; callers encode previews.

use std::path::Path;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

/// A decoded RGB frame uses three bytes per pixel.
#[derive(Clone)]
pub struct RgbFrame {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
    /// Original media position; zero for static images or the first frame.
    pub timestamp_ms: i64,
}

/// Media probe result.
#[derive(Debug, Clone, Default)]
pub struct MediaInfo {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    /// Average frames per second.
    pub fps: Option<f64>,
}

/// Probe metadata from FFmpeg's input summary without decoding media or requiring ffprobe.
pub fn probe(path: &Path) -> AppResult<MediaInfo> {
    let summary = input_summary(path)?;
    #[cfg(target_os = "windows")]
    if summary.contains("Video: prores_raw") {
        super::raw_video::ensure_raw_decoder()?;
    }
    Ok(parse_media_info(&summary))
}

/// Detect video tracks that WebKit may accept without raising an error but render
/// as audio-only/black video. The repair path can remux `hev1` to `hvc1` without
/// re-encoding the picture.
pub fn needs_webkit_compatibility(path: &Path) -> AppResult<bool> {
    let summary = input_summary(path)?;
    Ok(summary_needs_webkit_compatibility(&summary))
}

fn summary_needs_webkit_compatibility(summary: &str) -> bool {
    let Some(video) = summary.lines().find(|line| line.contains(" Video: ")) else {
        return false;
    };
    video.contains("(hev1 /")
        || (video.contains("Video: h264")
            && (video.contains("yuv420p10") || video.contains("yuv422p10")))
}

fn input_summary(path: &Path) -> AppResult<String> {
    let out = super::ffmpeg_command()
        .args(["-hide_banner", "-i"])
        .arg(path)
        .output()
        .map_err(|e| AppError::Msg(format!("ffmpeg probe spawn failed: {e}")))?;
    let summary = String::from_utf8_lossy(&out.stderr).into_owned();
    if summary.lines().any(|line| line.contains("Input #")) {
        Ok(summary)
    } else {
        Err(AppError::Msg(format!(
            "FFmpeg could not recognize the media file: {}",
            path.display()
        )))
    }
}

pub(super) fn parse_media_info(summary: &str) -> MediaInfo {
    let duration_ms = summary.lines().find_map(|line| {
        let value = line.split("Duration:").nth(1)?.split(',').next()?.trim();
        let mut parts = value.split(':');
        let hours: f64 = parts.next()?.parse().ok()?;
        let minutes: f64 = parts.next()?.parse().ok()?;
        let seconds: f64 = parts.next()?.parse().ok()?;
        Some(((hours * 3600.0 + minutes * 60.0 + seconds) * 1000.0).round() as i64)
    });
    let video = summary.lines().find(|line| line.contains(" Video: "));
    let (width, height) = video
        .and_then(|line| {
            line.split(|c: char| c == ',' || c.is_whitespace())
                .find_map(|token| {
                    let token = token.trim_matches(|c: char| !c.is_ascii_digit() && c != 'x');
                    let (width, height) = token.split_once('x')?;
                    let width = width.parse::<i64>().ok()?;
                    let height = height.parse::<i64>().ok()?;
                    // Exclude hexadecimal codec tags such as 0x31307661 from dimension
                    // parsing.
                    if !(1..=100_000).contains(&width) || !(1..=100_000).contains(&height) {
                        return None;
                    }
                    Some((Some(width), Some(height)))
                })
        })
        .unwrap_or((None, None));
    let fps = video.and_then(|line| {
        let tokens: Vec<_> = line.split_whitespace().collect();
        tokens
            .windows(2)
            .find(|pair| pair[1].trim_matches(',') == "fps")
            .and_then(|pair| pair[0].parse().ok())
    });
    MediaInfo {
        width,
        height,
        duration_ms,
        fps,
    }
}

/// Build FFmpeg commands with a shared binary path and arguments; builder methods borrow
/// mutably, so return ownership after construction.
fn command() -> ffmpeg_sidecar::command::FfmpegCommand {
    let mut cmd = ffmpeg_sidecar::command::FfmpegCommand::new_with_path(super::ffmpeg_path());
    cmd.hide_banner().no_audio();
    cmd
}

/// Extract a first-frame preview bounded by thumb_max.
pub fn extract_first_frame(path: &Path, thumb_max: u32) -> AppResult<RgbFrame> {
    let vf = format!("scale=min({m}\\,iw):-1", m = thumb_max);
    let iter = command()
        .input(path.to_string_lossy().as_ref())
        .args(["-vf", &vf])
        .frames(1)
        .rawvideo()
        .spawn()
        .map_err(|e| AppError::Msg(format!("ffmpeg spawn failed: {e}")))?
        .iter()
        .map_err(|e| AppError::Msg(format!("ffmpeg pipe failed: {e}")))?;
    if let Some(frame) = iter.filter_frames().next() {
        return Ok(RgbFrame {
            width: frame.width,
            height: frame.height,
            data: frame.data,
            timestamp_ms: 0,
        });
    }
    Err(AppError::Msg("no frames extracted from video".into()))
}

/// Encode an image or video's first frame as a thumbnail.
pub fn create_thumbnail_webp(path: &Path, dest: &Path, thumb_max: u32) -> AppResult<()> {
    let media = crate::embedding::media::Media {
        ffmpeg: super::ffmpeg_path(),
        ffprobe: crate::model_download::runtime().join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }),
    };
    let decoded = media
        .frame(path, 0., thumb_max, thumb_max, false)
        .map_err(|e| AppError::Msg(e.to_string()))?;
    crate::import::thumb::encode_dynamic(dest, &decoded)
}

#[derive(Debug, Clone)]
pub struct TimedFrameFile {
    pub timestamp_ms: i64,
    pub path: PathBuf,
}

/// Check whether a video can play directly in the WebView for hover previews.
pub fn is_web_playable(path: &Path) -> bool {
    let codec = video_codec(path).unwrap_or_default();
    matches!(
        codec.as_str(),
        "h264" | "hevc" | "h265" | "vp8" | "vp9" | "av1" | "mpeg4"
    ) && matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "m4v" | "webm" | "mov")
    )
}

pub(crate) fn video_codec(path: &Path) -> Option<String> {
    let summary = input_summary(path).ok()?;
    let video = summary.lines().find(|line| line.contains(" Video: "))?;
    Some(
        video
            .split(" Video: ")
            .nth(1)?
            .split(',')
            .next()?
            .split_whitespace()
            .next()?
            .into(),
    )
}

#[cfg(test)]
#[path = "../../tests/unit/ffmpeg.rs"]
mod tests;
