use super::{ensure_first_import_size, validate_first_import_files, FIRST_IMPORT_MAX_BYTES};
use crate::error::AppError;

fn file(dir: &std::path::Path, name: &str, size: u64) -> String {
    let path = dir.join(name);
    // Sparse files test byte boundaries without writing large media fixtures.
    std::fs::File::create(&path).unwrap().set_len(size).unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn accepts_ten_files_totaling_exactly_one_hundred_mb() {
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = (0..10)
        .map(|i| file(dir.path(), &format!("{i}.jpg"), 10_000_000))
        .collect();
    assert_eq!(validate_first_import_files(&paths).unwrap(), 100_000_000);
}

#[test]
fn rejects_aggregate_size_even_when_each_file_is_below_limit() {
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![
        file(dir.path(), "a.jpg", 50_000_000),
        file(dir.path(), "b.mp4", 50_000_001),
    ];
    let error = validate_first_import_files(&paths).unwrap_err();
    match error {
        AppError::FirstImportSizeExceeded {
            incoming_bytes,
            limit_bytes,
        } => {
            assert_eq!(incoming_bytes, 100_000_001);
            assert_eq!(limit_bytes, 100_000_000);
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(std::fs::metadata(&paths[1]).unwrap().len(), 50_000_001);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}

#[test]
fn rejects_one_oversized_video_and_accepts_a_smaller_reselection() {
    let dir = tempfile::tempdir().unwrap();
    let large = file(dir.path(), "large.mp4", 150_000_000);
    assert_eq!(
        validate_first_import_files(&[large]).unwrap_err().code(),
        "first_import_size_exceeded"
    );
    let small = file(dir.path(), "small.mp4", 25_000_000);
    assert_eq!(validate_first_import_files(&[small]).unwrap(), 25_000_000);
}

#[test]
fn rejects_empty_or_more_than_ten_files_before_inspecting_paths() {
    assert_eq!(
        validate_first_import_files(&[]).unwrap_err().code(),
        "first_import_count_limit"
    );
    assert_eq!(
        validate_first_import_files(&vec!["missing.jpg".into(); 11])
            .unwrap_err()
            .code(),
        "first_import_count_limit"
    );
}

#[test]
fn rejects_directories_and_unreadable_selections() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        validate_first_import_files(&[dir.path().to_string_lossy().into_owned()])
            .unwrap_err()
            .code(),
        "first_import_files_only"
    );
    assert_eq!(
        validate_first_import_files(&[dir
            .path()
            .join("missing.jpg")
            .to_string_lossy()
            .into_owned()])
        .unwrap_err()
        .code(),
        "io"
    );
}

#[test]
fn sample_download_accumulation_uses_the_same_inclusive_byte_boundary() {
    assert!(ensure_first_import_size(FIRST_IMPORT_MAX_BYTES).is_ok());
    assert_eq!(
        ensure_first_import_size(FIRST_IMPORT_MAX_BYTES + 1)
            .unwrap_err()
            .code(),
        "first_import_size_exceeded"
    );
}
