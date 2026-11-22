use super::*;

#[test]
fn nested_workers_block_cleanup_until_every_guard_is_released() {
    let activity = AssetActivity::default();
    let first = activity.work().unwrap();
    let second = activity.work().unwrap();
    assert!(activity.cleanup().is_err());
    drop(first);
    assert!(activity.cleanup().is_err());
    drop(second);
    let cleanup = activity.cleanup().unwrap();
    assert!(activity.work().is_err());
    assert!(activity.cleanup().is_err());
    drop(cleanup);
    assert!(activity.work().is_ok());
}

#[test]
fn guards_release_activity_during_worker_and_cleanup_unwinding() {
    let activity = AssetActivity::default();
    assert!(std::panic::catch_unwind(|| {
        let _guard = activity.work().unwrap();
        panic!("worker failed");
    })
    .is_err());
    assert!(std::panic::catch_unwind(|| {
        let _guard = activity.cleanup().unwrap();
        panic!("cleanup failed");
    })
    .is_err());
    assert!(activity.work().is_ok());
    assert!(activity.cleanup().is_ok());
}

#[test]
fn aliases_share_activity_but_another_library_can_be_cleaned_independently() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    let activity = for_root(first.path()).unwrap();
    let alias = for_root(&first.path().join(".")).unwrap();
    let other = for_root(second.path()).unwrap();
    let _worker = activity.work().unwrap();
    assert!(alias.cleanup().is_err());
    assert!(other.cleanup().is_ok());
}

#[test]
fn migration_refuses_active_file_users_before_changing_files_or_registry() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    let meta = crate::library::init_library(&source, "Migration activity").unwrap();
    let mut config =
        crate::config::GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
    config.libraries.push(meta.clone());
    config.save().unwrap();
    let activity = for_root(&source).unwrap();
    let _reader = activity.work().unwrap();
    let result = crate::library::migrate_library(&mut config, &meta.id, target.to_str().unwrap());
    assert!(
        result.is_err(),
        "migration must not remove a live reader's library"
    );
    assert!(source.is_dir());
    assert!(!target.exists());
    assert_eq!(config.libraries[0].path, meta.path);
    assert!(config.pending_migrations.is_empty());
}

#[test]
fn completed_migration_retires_old_sessions_and_preserves_independent_new_activity() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let target = temp.path().join("target");
    let meta = crate::library::init_library(&source, "Migration retirement").unwrap();
    let mut config =
        crate::config::GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
    config.libraries.push(meta.clone());
    let activity = for_root(&source).unwrap();
    crate::library::migrate_library_with_progress(
        &mut config,
        &meta.id,
        target.to_str().unwrap(),
        &|_, _, _| {
            assert!(activity.work().is_err());
        },
    )
    .unwrap();
    assert!(
        activity.work().is_err(),
        "a stale session cannot recreate directories after migration"
    );
    assert!(for_root(&target).unwrap().work().is_ok());
    assert!(!source.exists());
}

#[test]
fn opening_a_second_session_cannot_recover_over_an_active_worker() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Session activity").unwrap();
    let session = crate::library::open_session(meta.clone()).unwrap();
    let worker = session.asset_activity.work().unwrap();
    assert!(crate::library::open_session_light(meta.clone()).is_err());
    drop(worker);
    assert!(crate::library::open_session_light(meta).is_ok());
}
