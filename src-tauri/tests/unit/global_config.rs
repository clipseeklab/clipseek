use super::*;

fn pending_config(temp: &tempfile::TempDir) -> (GlobalConfig, PathBuf, PathBuf, PathBuf) {
    let config_path = temp.path().join("config.json");
    let old = temp.path().join("old-library");
    let new = temp.path().join("new-library");
    fs::create_dir_all(&old).unwrap();
    let mut config = GlobalConfig::load_from(config_path.clone()).unwrap();
    config.libraries.push(LibraryMeta {
        id: "library-1".into(),
        name: "test".into(),
        path: old.to_string_lossy().into_owned(),
        created_at: 1,
        color: None,
    });
    config.pending_migrations.push(PendingMigration {
        library_id: "library-1".into(),
        old_path: old.to_string_lossy().into_owned(),
        new_path: new.to_string_lossy().into_owned(),
    });
    (config, config_path, old, new)
}

#[test]
fn interrupted_migration_uses_new_location_after_directory_switch() {
    let temp = tempfile::tempdir().unwrap();
    let (config, path, old, new) = pending_config(&temp);
    fs::rename(old, &new).unwrap();
    config.save().unwrap();

    let recovered = GlobalConfig::load_from(path.clone()).unwrap();
    assert_eq!(recovered.libraries[0].path, new.to_string_lossy());
    assert!(recovered.pending_migrations.is_empty());
    assert!(GlobalConfig::load_from(path)
        .unwrap()
        .pending_migrations
        .is_empty());
}

#[test]
fn interrupted_migration_keeps_old_location_before_directory_switch() {
    let temp = tempfile::tempdir().unwrap();
    let (config, path, old, _new) = pending_config(&temp);
    config.save().unwrap();

    let recovered = GlobalConfig::load_from(path).unwrap();
    assert_eq!(recovered.libraries[0].path, old.to_string_lossy());
    assert!(recovered.pending_migrations.is_empty());
}

#[test]
fn legacy_config_with_library_skips_onboarding() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.json");
    let raw = serde_json::json!({
        "version": 1,
        "libraries": [{
            "id": "legacy",
            "name": "Legacy",
            "path": temp.path().join("library").to_string_lossy(),
            "createdAt": 1
        }],
        "activeLibraryId": "legacy",
        "settings": {},
        "pendingMigrations": []
    });
    fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();

    let config = GlobalConfig::load_from(path).unwrap();
    assert!(config.onboarding.completed);
    assert_eq!(config.onboarding.version, ONBOARDING_VERSION);
}

#[test]
fn legacy_empty_config_starts_onboarding() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.json");
    let raw = serde_json::json!({
        "version": 1,
        "libraries": [],
        "activeLibraryId": null,
        "settings": {},
        "pendingMigrations": []
    });
    fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();

    let config = GlobalConfig::load_from(path).unwrap();
    assert!(!config.onboarding.completed);
    assert_eq!(config.onboarding.current_step, 1);
}

#[test]
fn four_step_onboarding_at_search_migrates_to_completed() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.json");
    let raw = serde_json::json!({
        "version": 1,
        "libraries": [],
        "activeLibraryId": null,
        "settings": {},
        "onboarding": { "version": 1, "currentStep": 4, "completed": false },
        "pendingMigrations": []
    });
    fs::write(&path, serde_json::to_string(&raw).unwrap()).unwrap();

    let config = GlobalConfig::load_from(path).unwrap();
    assert!(config.onboarding.completed);
    assert_eq!(config.onboarding.current_step, 3);
    assert_eq!(config.onboarding.version, ONBOARDING_VERSION);
}

#[test]
fn startup_failure_cannot_overwrite_the_original_configuration() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(&path, b"{broken configuration").unwrap();
    let config = GlobalConfig::startup_from(path.clone());
    assert!(config.save().is_err(), "a failed load must block writes");
    assert_eq!(fs::read(path).unwrap(), b"{broken configuration");
}

#[test]
fn startup_can_retry_after_configuration_is_repaired_without_restarting() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("config.json");
    fs::write(&path, b"not json").unwrap();
    let mut config = GlobalConfig::startup_from(path.clone());
    assert!(config.ensure_loaded().is_err());
    fs::write(&path, serde_json::to_vec(&GlobalConfig::default()).unwrap()).unwrap();
    config.ensure_loaded().unwrap();
    config.settings.theme = "dark".into();
    config.save().unwrap();
    assert_eq!(
        GlobalConfig::load_from(path).unwrap().settings.theme,
        "dark"
    );
}
