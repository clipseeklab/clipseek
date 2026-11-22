//! Volume tests must recognize and accept local directories.

use clip_studio_lib::volume;

#[test]
fn local_directory_is_not_network_volume() {
    let temp = tempfile::tempdir().unwrap();
    assert!(!volume::is_network_volume(temp.path()));
    // Inspect ancestors when the directory has not been created.
    assert!(!volume::is_network_volume(
        &temp.path().join("sub").join("lib")
    ));
    assert!(volume::volume_type_name(temp.path()).is_some());
}

#[test]
fn ensure_local_volume_accepts_local_paths() {
    let temp = tempfile::tempdir().unwrap();
    volume::ensure_local_volume(temp.path()).unwrap();
}

#[cfg(target_os = "windows")]
#[test]
fn canonicalized_local_library_path_is_not_a_network_share() {
    let temp = tempfile::tempdir().unwrap();
    let canonical = std::fs::canonicalize(temp.path()).unwrap();
    assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
    assert!(!volume::is_network_volume(&canonical), "{canonical:?}");
    assert!(!volume::is_network_volume(
        &canonical.join("future-library")
    ));
    volume::ensure_local_volume(&canonical).unwrap();
}
