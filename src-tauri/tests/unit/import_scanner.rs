use super::*;
use std::path::Path;

#[test]
fn classifies_new_image_formats_case_insensitively() {
    for path in ["art.svg", "photo.HEIC", "still.HeIf"] {
        assert_eq!(classify(Path::new(path)), Some(Kind::Image));
    }
}

#[test]
fn maps_new_image_mime_types() {
    assert_eq!(mime_of(Path::new("art.svg")), "image/svg+xml");
    assert_eq!(mime_of(Path::new("photo.heic")), "image/heic");
    assert_eq!(mime_of(Path::new("photo.HEIF")), "image/heif");
}

#[test]
fn shallow_scan_ignores_nested_directories() {
    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("top.jpg"), b"top").unwrap();
    let nested = temp.path().join("nested");
    std::fs::create_dir(&nested).unwrap();
    std::fs::write(nested.join("child.jpg"), b"child").unwrap();

    let files = scan_shallow_dir(temp.path());
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].file_name().unwrap(), "top.jpg");
}
