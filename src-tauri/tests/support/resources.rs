use std::path::PathBuf;

/// Use explicit local fixture paths; optional media stays outside version control.
pub fn media_dir(kind: &str) -> PathBuf {
    let specific = match kind {
        "raw-video" => Some("CLIPSTUDIO_RAW_VIDEO_TEST_DATA"),
        "prores" => Some("CLIPSTUDIO_PRORES_TEST_DATA"),
        _ => None,
    };
    if let Some(path) = specific.and_then(std::env::var_os) {
        return PathBuf::from(path);
    }
    let root = std::env::var_os("CLIPSTUDIO_TEST_DATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.local/test-media"));
    root.join(kind)
}
