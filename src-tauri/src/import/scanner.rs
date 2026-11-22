//! Import path scanning: recursively expand directories and filter extensions.

use std::path::PathBuf;

use walkdir::WalkDir;

/// Media kind selects the processing branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Image,
    Video,
    /// Animated images use FFmpeg decoding in the video branch.
    Gif,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Image => "image",
            Kind::Video => "video",
            Kind::Gif => "gif",
        }
    }
}

/// Identify supported media kinds by extension.
pub fn classify(path: &std::path::Path) -> Option<Kind> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())?;
    match ext.as_str() {
        "jpg" | "jpeg" | "png" | "webp" | "bmp" | "svg" | "heic" | "heif" => Some(Kind::Image),
        "gif" => Some(Kind::Gif),
        "mp4" | "mov" | "webm" | "mkv" | "avi" | "m4v" => Some(Kind::Video),
        _ => None,
    }
}

/// Map extension to MIME type.
pub fn mime_of(path: &std::path::Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "gif" => "image/gif",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "avi" => "video/x-msvideo",
        "flv" => "video/x-flv",
        "ts" | "m2ts" => "video/mp2t",
        "3gp" | "3gpp" => "video/3gpp",
        "3g2" | "3gpp2" => "video/3gpp2",
        "ogv" => "video/ogg",
        "mpg" | "mpeg" => "video/mpeg",
        _ => "application/octet-stream",
    }
    .to_string()
}

#[cfg(test)]
#[path = "../../tests/unit/import_scanner.rs"]
mod tests;

/// Recursively expand directories and retain supported files.
pub fn scan(paths: &[PathBuf]) -> Vec<PathBuf> {
    scan_report(paths).0
}

/// Keep inaccessible/missing inputs alongside discovered materials for the durable queue.
pub fn scan_report(paths: &[PathBuf]) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut out = Vec::new();
    let mut issues = Vec::new();
    for p in paths {
        if p.is_dir() {
            for entry in WalkDir::new(p).follow_links(false).max_depth(16) {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        issues.push((error.path().unwrap_or(p).to_owned(), error.to_string()));
                        continue;
                    }
                };
                let path = entry.path();
                if path.is_file() && classify(path).is_some() {
                    out.push(path.to_path_buf());
                }
            }
        } else if p.is_file() && classify(p).is_some() {
            out.push(p.clone());
        } else if !p.exists() || std::fs::metadata(p).is_err() {
            let error = std::fs::metadata(p)
                .err()
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Source unavailable".into());
            issues.push((p.clone(), error));
        }
    }
    // Overlapping dropped paths must not schedule the same file twice in one batch.
    let mut seen = std::collections::HashSet::new();
    out.retain(|path| seen.insert(path.clone()));
    (out, issues)
}

/// Scan only supported immediate children of a directory.
pub fn scan_shallow_dir(dir: &std::path::Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && classify(path).is_some())
        .collect()
}

/// Compute a streaming BLAKE3 hash.
pub fn file_hash(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
