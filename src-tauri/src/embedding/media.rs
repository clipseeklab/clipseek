use crate::embedding::domain::validate_duration;
use anyhow::{bail, Context, Result};
use image::{DynamicImage, RgbImage};
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone)]
pub struct Media {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

impl Media {
    pub fn duration(&self, path: &Path) -> Result<f64> {
        let output = command(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-show_entries",
                "format=duration",
                "-of",
                "default=noprint_wrappers=1:nokey=1",
            ])
            .arg(path)
            .output()?;
        if !output.status.success() {
            bail!("Could not read video duration");
        }
        let duration: f64 = String::from_utf8_lossy(&output.stdout).trim().parse()?;
        validate_duration(duration)?;
        Ok(duration)
    }
    pub fn video_info(&self, path: &Path) -> Result<(f64, u32, u32)> {
        let output = command(&self.ffprobe)
            .args([
                "-v",
                "error",
                "-show_format",
                "-show_streams",
                "-of",
                "json",
            ])
            .arg(path)
            .output()?;
        if !output.status.success() {
            bail!("Could not read video information");
        }
        let probe: Value = serde_json::from_slice(&output.stdout)?;
        let stream = probe["streams"]
            .as_array()
            .and_then(|streams| streams.iter().find(|s| s["codec_type"] == "video"))
            .context("No video track")?;
        let duration: f64 = probe["format"]["duration"]
            .as_str()
            .context("Could not determine video duration")?
            .parse()?;
        validate_duration(duration)?;
        let mut width = stream["width"].as_u64().context("Missing video width")? as u32;
        let mut height = stream["height"].as_u64().context("Missing video height")? as u32;
        // FFmpeg autorotates display matrices; match its decoded orientation.
        let rotation = stream["side_data_list"]
            .as_array()
            .and_then(|list| list.iter().find_map(|item| item["rotation"].as_i64()))
            .unwrap_or(0);
        if rotation.rem_euclid(180) == 90 {
            std::mem::swap(&mut width, &mut height);
        }
        Ok((duration, width, height))
    }
    pub fn frame(
        &self,
        path: &Path,
        second: f64,
        width: u32,
        height: u32,
        exact: bool,
    ) -> Result<DynamicImage> {
        let filter = if exact {
            format!("scale={width}:{height}:flags=bicubic")
        } else {
            format!("scale={width}:{height}:force_original_aspect_ratio=decrease")
        };
        let mut child = command(&self.ffmpeg)
            .args([
                "-v",
                "error",
                "-nostdin",
                "-ss",
                &format!("{second:.6}"),
                "-i",
            ])
            .arg(path)
            .args([
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                &filter,
                "-f",
                "image2pipe",
                "-vcodec",
                "png",
                "pipe:1",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("Could not start FFmpeg; prepare the native runtime")?;
        let mut bytes = Vec::new();
        let read = child
            .stdout
            .take()
            .context("No decoded video output")?
            .take(32 * 1024 * 1024)
            .read_to_end(&mut bytes);
        if read.is_err() || bytes.len() >= 32 * 1024 * 1024 {
            let _ = child.kill();
        }
        let status = child.wait()?;
        read?;
        if !status.success() || bytes.is_empty() {
            bail!("Video decoding failed");
        }
        Ok(image::load_from_memory(&bytes)?)
    }
    pub fn model_frame(
        &self,
        path: &Path,
        second: f64,
        width: u32,
        height: u32,
    ) -> Result<RgbImage> {
        Ok(self.frame(path, second, width, height, true)?.to_rgb8())
    }
}

pub fn command(binary: &Path) -> Command {
    let command = Command::new(binary);
    #[cfg(windows)]
    let command = {
        let mut command = command;
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
        command
    };
    command
}
