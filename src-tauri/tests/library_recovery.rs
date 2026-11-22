use clip_studio_lib::{config::GlobalConfig, library};

#[test]
fn failed_registration_save_keeps_the_old_library_available_for_cleanup_retry() {
    let dir = tempfile::tempdir().unwrap();
    let old = library::init_library(&dir.path().join("old"), "Old").unwrap();
    let replacement = library::init_library(&dir.path().join("new"), "New").unwrap();
    let blocker = dir.path().join("blocked");
    let mut config = GlobalConfig::load_from(blocker.join("config.json")).unwrap();
    config.libraries = vec![old.clone(), replacement.clone()];
    config.active_library_id = Some(replacement.id.clone());
    std::fs::write(&blocker, b"not a directory").unwrap();
    assert!(library::delete_library(&mut config, &old.id, false).is_err());
    assert!(
        config.libraries.iter().any(|item| item.id == old.id),
        "save failure must preserve the registration in memory"
    );
    assert_eq!(
        config.active_library_id.as_deref(),
        Some(replacement.id.as_str())
    );
    assert!(std::path::Path::new(&old.path)
        .join(".clipstudio-open/library.db")
        .is_file());
}

#[test]
fn removing_failed_registration_preserves_files_other_libraries_and_onboarding_after_restart() {
    let dir = tempfile::tempdir().unwrap();
    let old = library::init_library(&dir.path().join("old"), "Old").unwrap();
    let replacement = library::init_library(&dir.path().join("new"), "New").unwrap();
    let path = dir.path().join("config.json");
    let mut config = GlobalConfig::load_from(path.clone()).unwrap();
    config.libraries = vec![old.clone(), replacement.clone()];
    config.active_library_id = Some(replacement.id.clone());
    config.onboarding.completed = true;
    config.save().unwrap();
    library::delete_library(&mut config, &old.id, false).unwrap();
    let restored = GlobalConfig::load_from(path).unwrap();
    assert_eq!(restored.libraries.len(), 1);
    assert_eq!(
        restored.active_library_id.as_deref(),
        Some(replacement.id.as_str())
    );
    assert!(restored.onboarding.completed);
    for meta in [&old, &replacement] {
        assert!(std::path::Path::new(&meta.path)
            .join(".clipstudio-open/library.db")
            .is_file());
    }
}

#[test]
fn deleting_an_already_missing_registration_never_recreates_its_directory() {
    let dir = tempfile::tempdir().unwrap();
    let mut old = library::init_library(&dir.path().join("old"), "Old").unwrap();
    old.path = dir.path().join("missing").to_string_lossy().into_owned();
    let replacement = library::init_library(&dir.path().join("new"), "New").unwrap();
    let mut config = GlobalConfig::load_from(dir.path().join("config.json")).unwrap();
    config.libraries = vec![old.clone(), replacement.clone()];
    config.active_library_id = Some(replacement.id.clone());
    library::delete_library(&mut config, &old.id, false).unwrap();
    assert!(!std::path::Path::new(&old.path).exists());
    assert_eq!(config.libraries.len(), 1);
}

#[test]
fn explicit_file_deletion_keeps_other_libraries_and_persists_the_fallback() {
    let dir = tempfile::tempdir().unwrap();
    let old = library::init_library(&dir.path().join("old"), "Old").unwrap();
    let replacement = library::init_library(&dir.path().join("new"), "New").unwrap();
    let config_path = dir.path().join("config.json");
    let mut config = GlobalConfig::load_from(config_path.clone()).unwrap();
    config.libraries = vec![old.clone(), replacement.clone()];
    config.active_library_id = Some(old.id.clone());
    let resolved = std::fs::canonicalize(&old.path).unwrap();
    assert!(resolved.starts_with(std::fs::canonicalize(dir.path()).unwrap()));
    library::delete_library(&mut config, &old.id, true).unwrap();
    assert!(!std::path::Path::new(&old.path).exists());
    assert!(std::path::Path::new(&replacement.path)
        .join(".clipstudio-open/library.db")
        .is_file());
    let restored = GlobalConfig::load_from(config_path).unwrap();
    assert_eq!(
        restored.active_library_id.as_deref(),
        Some(replacement.id.as_str())
    );
}
