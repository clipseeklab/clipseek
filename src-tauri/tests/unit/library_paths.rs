use super::*;

#[test]
fn rejects_empty_library_name_before_creating_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("new-library");
    assert!(init_library(&root, "  ").is_err());
    assert!(!root.exists());
}

#[cfg(target_os = "windows")]
#[test]
fn rejects_reserved_windows_library_directory_before_creating_files() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("CON.txt");
    assert!(init_library(&root, "valid").is_err());
    assert!(!root.exists());
}

#[test]
fn rejects_nonportable_library_names_before_creating_files() {
    for name in [
        "Bad:Name",
        "a/b",
        "a\\b",
        "CON",
        "com1.txt",
        "LPT¹.txt",
        "name.",
        "name ",
        "line\nname",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("new-library");
        assert!(
            init_library(&root, name).is_err(),
            "accepted invalid name: {name:?}"
        );
        assert!(!root.exists(), "created files for invalid name: {name:?}");
    }
}

#[test]
fn portable_library_name_limit_counts_utf8_bytes() {
    assert!(validate_library_name(&"中".repeat(85)).is_ok());
    assert_eq!(
        validate_library_name(&"中".repeat(86)).unwrap_err().code(),
        "library_name_too_long"
    );
    assert!(validate_library_name("旅行：灵感").is_ok());
    assert!(validate_library_name("CONtent").is_ok());
    assert!(validate_library_name("com10").is_ok());
}
