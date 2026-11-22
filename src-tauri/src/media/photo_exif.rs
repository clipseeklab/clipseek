//! Best-effort, read-only EXIF extraction independent of FFmpeg's dictionaries.
use super::metadata::MetadataTag;
use exif::{In, Reader, Tag, Value};
use std::{
    fs::File,
    io::{self, BufReader, Read, Seek, SeekFrom},
    path::Path,
};

const MAX_READ: u64 = 16 * 1024 * 1024;
const MAX_TAGS: usize = 256;

// TIFF readers can consume the entire source, and other containers may place
// EXIF after pixel data. Bound cumulative reads even when HEIF seeks backwards.
struct BudgetReader<R> {
    source: R,
    remaining: u64,
}
impl<R: Read> Read for BudgetReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        if self.remaining == 0 {
            return Err(io::Error::other("EXIF read budget exceeded"));
        }
        let limit = buffer.len().min(self.remaining as usize);
        let count = self.source.read(&mut buffer[..limit])?;
        self.remaining -= count as u64;
        Ok(count)
    }
}
impl<R: Seek> Seek for BudgetReader<R> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.source.seek(position)
    }
}

pub(super) fn read(path: &Path) -> Vec<MetadataTag> {
    let Ok(file) = File::open(path) else {
        return vec![];
    };
    let mut source = BufReader::new(BudgetReader {
        source: file,
        remaining: MAX_READ,
    });
    let parsed = Reader::new()
        .continue_on_error(true)
        .read_from_container(&mut source)
        .or_else(|error| error.distill_partial_result(|_| {}));
    let Ok(exif) = parsed else {
        // Missing, unsupported or malformed EXIF must not hide basic media info.
        return vec![];
    };
    exif.fields()
        .take(MAX_TAGS)
        .map(|field| {
            let name = if field.ifd_num == In::PRIMARY {
                field.tag.to_string()
            } else {
                format!("IFD{}.{}", field.ifd_num.index(), field.tag)
            };
            let value = match &field.value {
                Value::Ascii(values) => values
                    .iter()
                    .map(|bytes| String::from_utf8_lossy(bytes))
                    .collect::<Vec<_>>()
                    .join("; "),
                // MakerNote and other opaque binary data have no portable
                // interpretation; do not send thousands of raw bytes to UI.
                Value::Undefined(bytes, _) if field.tag == Tag::MakerNote || bytes.len() > 1024 => {
                    format!("{} bytes", bytes.len())
                }
                _ => field.display_value().with_unit(&exif).to_string(),
            };
            MetadataTag {
                name: name.chars().take(128).collect(),
                value: value
                    .chars()
                    .filter(|c| !c.is_control() || *c == '\n' || *c == '\t')
                    .take(4096)
                    .collect(),
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "../../tests/unit/photo_exif.rs"]
mod tests;
