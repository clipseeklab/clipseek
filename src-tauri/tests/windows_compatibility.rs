#![cfg(target_os = "windows")]
#[path = "support/resources.rs"]
mod test_resources;

use std::path::PathBuf;

use clip_studio_lib::{import::thumb, library, logging};

fn heif_sample(variable: &str, filename: &str) -> Option<PathBuf> {
    let path = std::env::var_os(variable)
        .map(PathBuf::from)
        .unwrap_or_else(|| test_resources::media_dir("heic").join(filename));
    clip_studio_lib::util::test_prerequisite(
        path.is_file(),
        &format!("HEIF sample missing: {filename}"),
    )
    .then_some(path)
}

#[test]
fn logs_go_to_local_app_data() {
    let local = std::env::var_os("LOCALAPPDATA").unwrap();
    assert_eq!(
        logging::log_dir(),
        PathBuf::from(local).join("org.clipstudio.open/logs")
    );
}

#[test]
fn rejects_reserved_library_directory_before_creating_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("CON.txt");
    assert!(library::init_library(&root, "valid").is_err());
    assert!(!root.exists());
}

#[test]
fn decodes_heic_and_heif_with_bundled_decoder() {
    let Some(sample) = heif_sample("CLIPSEEK_HEIF_TEST_SAMPLE", "sample.heic") else {
        return;
    };
    let temp = tempfile::tempdir().unwrap();
    let mut dimensions = None;
    for extension in ["heic", "heif"] {
        let path = temp.path().join(format!("中文素材.{extension}"));
        std::fs::copy(&sample, &path).unwrap();
        let probe = thumb::image_thumb(&path).unwrap();
        assert!(probe.width > 0 && probe.height > 0);
        if let Some(expected) = dimensions {
            assert_eq!((probe.width, probe.height), expected);
        }
        dimensions = Some((probe.width, probe.height));
        assert!(probe.thumb.is_some());
        let preview = thumb::full_heif_preview_png(&path).unwrap();
        let image = image::load_from_memory_with_format(&preview, image::ImageFormat::Png).unwrap();
        assert_eq!(
            (i64::from(image.width()), i64::from(image.height())),
            (probe.width, probe.height)
        );
    }
}

#[test]
fn decodes_complete_tiled_heic_primary_image() {
    let Some(sample) = heif_sample(
        "CLIPSEEK_HEIF_GRID_TEST_SAMPLE",
        "synthetic-grid-2560x1440.heic",
    ) else {
        return;
    };
    let probe = thumb::image_thumb(&sample).unwrap();
    assert_eq!((probe.width, probe.height), (2560, 1440));
    let preview = thumb::full_heif_preview_png(&sample).unwrap();
    let image = image::load_from_memory_with_format(&preview, image::ImageFormat::Png).unwrap();
    assert_eq!((image.width(), image.height()), (2560, 1440));
}

#[test]
fn rejects_corrupt_heif_without_leaving_a_preview() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("corrupt.heif");
    std::fs::write(&source, b"not an HEIF image").unwrap();
    assert!(thumb::image_thumb(&source).is_err());
}
