//! Read display metadata without decoding frames or adding an FFprobe dependency.
use crate::error::{AppError, AppResult};
use serde::Serialize;
use std::{
    io::{Read, Seek, SeekFrom},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_OUTPUT: u64 = 1024 * 1024;
const MAX_TAGS: usize = 256;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaMetadata {
    pub format: Option<String>,
    pub bit_rate: Option<u64>,
    pub created_at: Option<u64>,
    pub modified_at: Option<u64>,
    pub tags: Vec<MetadataTag>,
    pub exif: Vec<MetadataTag>,
    pub streams: Vec<StreamMetadata>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataTag {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StreamMetadata {
    pub index: u32,
    pub kind: String,
    pub codec: String,
    pub profile: Option<String>,
    pub language: Option<String>,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub frame_rate: Option<f64>,
    pub bit_rate: Option<u64>,
    pub pixel_format: Option<String>,
    pub sample_rate: Option<u32>,
    pub channel_layout: Option<String>,
    pub sample_format: Option<String>,
    pub description: String,
    pub tags: Vec<MetadataTag>,
}

fn failed(message: &str) -> AppError {
    AppError::coded("asset_metadata_failed", message)
}

fn seconds(time: std::io::Result<SystemTime>) -> Option<u64> {
    time.ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

pub fn read(path: &Path) -> AppResult<MediaMetadata> {
    let file = path.metadata()?;
    if !file.is_file() {
        return Err(failed("Asset source is not a file"));
    }
    let mut command = super::ffmpeg_command();
    command
        .args([
            "-hide_banner",
            "-nostdin",
            "-loglevel",
            "info",
            "-protocol_whitelist",
            "file,crypto,data",
            "-analyzeduration",
            "5000000",
            "-probesize",
            "5000000",
            "-i",
        ])
        .arg(path);
    let exif = super::photo_exif::read(path);
    let mut result =
        match input_summary(command, Duration::from_secs(8)).and_then(|summary| parse(&summary)) {
            Ok(result) => result,
            // EXIF remains useful for photos whose encoding FFmpeg cannot probe.
            Err(_) if !exif.is_empty() => MediaMetadata::default(),
            Err(error) => return Err(error),
        };
    result.exif = exif;
    result.created_at = seconds(file.created());
    result.modified_at = seconds(file.modified());
    Ok(result)
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn input_summary(mut command: Command, budget: Duration) -> AppResult<String> {
    // A bounded diagnostic spool avoids full pipe buffers and unbounded captured output.
    let mut output = tempfile::tempfile()?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(output.try_clone()?);
    let mut child = Process(command.spawn()?);
    let started = Instant::now();
    loop {
        if output.metadata()?.len() > MAX_OUTPUT || started.elapsed() > budget {
            return Err(failed("Media metadata exceeded size or time limits"));
        }
        if child.0.try_wait()?.is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    output.seek(SeekFrom::Start(0))?;
    let mut bytes = vec![];
    output.take(MAX_OUTPUT + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_OUTPUT {
        return Err(failed("Media metadata exceeded size limit"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn bounded(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
        .take(limit)
        .collect()
}

fn bit_rate(text: &str) -> Option<u64> {
    let mut words = text.split_whitespace();
    let value: f64 = words.next()?.parse().ok()?;
    let multiplier = match words.next()?.trim_end_matches(',') {
        "b/s" => 1.,
        "kb/s" => 1000.,
        "Mb/s" => 1_000_000.,
        _ => return None,
    };
    let bits = value * multiplier;
    (bits.is_finite() && bits > 0. && bits <= u64::MAX as f64).then_some(bits as u64)
}

fn properties(text: &str) -> Vec<&str> {
    let (mut depth, mut start) = (0_u32, 0);
    let mut parts = vec![];
    for (i, ch) in text.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                parts.push(text[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(text[start..].trim());
    parts
}

fn stream(line: &str) -> Option<StreamMetadata> {
    let prefix = line.trim().strip_prefix("Stream #0:")?;
    let index: u32 = prefix
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()?;
    let (kind, data) = ["Video", "Audio", "Subtitle", "Data", "Attachment"]
        .into_iter()
        .find_map(|kind| {
            line.split_once(&format!(": {kind}: "))
                .map(|(_, data)| (kind, data))
        })?;
    let parts = properties(data);
    let header = parts.first()?;
    let codec = header.split_whitespace().next()?.to_owned();
    let profile = header
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(profile, _)| profile)
        .filter(|v| !v.contains('/') && !v.contains("0x"))
        .map(|value| bounded(value, 128));
    let language = prefix
        .split(": ")
        .next()?
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(')'))
        .map(|(language, _)| bounded(language, 32));
    let mut result = StreamMetadata {
        index,
        kind: kind.to_ascii_lowercase(),
        codec,
        profile,
        language,
        bit_rate: parts.iter().find_map(|part| bit_rate(part)),
        description: bounded(data, 4096),
        ..Default::default()
    };
    if kind == "Video" {
        let info = super::ffmpeg::parse_media_info(line);
        result.width = info.width;
        result.height = info.height;
        result.frame_rate = info.fps.filter(|fps| fps.is_finite() && *fps > 0.);
        result.pixel_format = parts.get(1).map(|p| bounded(p, 256));
    } else if kind == "Audio" {
        if let Some((i, rate)) = parts.iter().enumerate().find_map(|(i, part)| {
            part.strip_suffix(" Hz")
                .and_then(|n| n.parse::<u32>().ok())
                .map(|rate| (i, rate))
        }) {
            result.sample_rate = (rate > 0).then_some(rate);
            result.channel_layout = parts.get(i + 1).map(|v| bounded(v, 128));
            result.sample_format = parts
                .get(i + 2)
                .filter(|v| !v.contains("b/s"))
                .map(|v| bounded(v, 128));
        }
    }
    Some(result)
}

fn parse(summary: &str) -> AppResult<MediaMetadata> {
    let mut result = MediaMetadata::default();
    let (mut current, mut tag_indent, mut count) = (None, None, 0);
    let mut recognized = false;
    for line in summary.lines() {
        let text = line.trim();
        let indent = line.len() - line.trim_start().len();
        if let Some(rest) = text.strip_prefix("Input #0, ") {
            result.format = rest
                .split_once(", from ")
                .map(|(format, _)| bounded(format, 256));
            recognized = true;
            current = None;
            tag_indent = None;
        } else if let Some(value) = stream(line) {
            tag_indent = None;
            if result.streams.len() < 64 {
                result.streams.push(value);
                current = Some(result.streams.len() - 1);
            } else {
                break;
            }
        } else if text.starts_with("Duration:") {
            result.bit_rate = text
                .split_once("bitrate:")
                .and_then(|(_, rate)| bit_rate(rate.trim()));
            tag_indent = None;
        } else if text == "Metadata:" || (text == "Side data:" && current.is_some()) {
            tag_indent = Some(indent);
        } else if let Some(level) = tag_indent {
            if indent <= level {
                tag_indent = None;
                continue;
            }
            let tags = match current {
                Some(index) => &mut result.streams[index].tags,
                None => &mut result.tags,
            };
            if let Some((name, value)) = text.split_once(':') {
                let name = name.trim();
                if name.is_empty() {
                    if let Some(last) = tags.last_mut() {
                        let room = 4096_usize.saturating_sub(last.value.chars().count() + 1);
                        if room > 0 {
                            last.value.push('\n');
                            last.value.push_str(&bounded(value.trim(), room));
                        }
                    }
                } else if count < MAX_TAGS {
                    tags.push(MetadataTag {
                        name: bounded(name, 128),
                        value: bounded(value.trim(), 4096),
                    });
                    count += 1;
                }
            }
        }
    }
    if !recognized {
        return Err(failed("No readable media metadata"));
    }
    Ok(result)
}

#[cfg(test)]
#[path = "../../tests/unit/media_metadata.rs"]
mod tests;
