use super::*;

#[cfg(target_os = "windows")]
#[test]
fn invalid_log_directory_is_not_reported_as_opened() {
    let temp = tempfile::tempdir().unwrap();
    assert!(open_log_directory(&temp.path().join("missing")).is_err());
}
