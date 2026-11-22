use std::time::{SystemTime, UNIX_EPOCH};

/// Generate beside the destination and publish only after a successful write.
/// Readers keep the previous complete file while encoding is in progress.
pub fn replace_file(
    path: &std::path::Path,
    write: impl FnOnce(&std::path::Path) -> crate::error::AppResult<()>,
) -> crate::error::AppResult<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    std::fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".clip-preview-")
        .suffix(".webp")
        .tempfile_in(parent)?
        .into_temp_path();
    write(&temporary)?;
    temporary
        .persist(path)
        .map_err(|e| crate::error::AppError::Io(e.error))?;
    Ok(())
}

/// Check external test prerequisites. Local runs may report skipped resources; strict CI mode
/// fails when required models, media, or FFmpeg are absent.
pub fn test_prerequisite(available: bool, reason: &str) -> bool {
    if available {
        return true;
    }
    if std::env::var("CLIPSTUDIO_CI_REQUIRE_RESOURCES").as_deref() == Ok("1") {
        panic!("Required compatibility test resources are missing: {reason}");
    }
    eprintln!("SKIPPED: {reason}");
    false
}

/// Generate an approximately time-ordered stable entity ID preserved across libraries.
pub fn new_entity_uid() -> String {
    uuid::Uuid::now_v7().to_string()
}

/// Current Unix timestamp in seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Generate a hyphen-free UUIDv4 string.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Convert a path to a forward-slash library-relative path.
pub fn to_rel(root: &std::path::Path, p: &std::path::Path) -> String {
    p.strip_prefix(root)
        .unwrap_or(p)
        .to_string_lossy()
        .replace('\\', "/")
}
