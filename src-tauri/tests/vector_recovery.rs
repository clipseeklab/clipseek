use std::fs;

use clip_studio_lib::library::{self, VectorHealth};
use clip_studio_lib::vector::recovery::{
    preflight, recover_interrupted_swap, RebuildJournal, RebuildPhase,
};

#[test]
fn rebuild_rejects_assets_that_failed_before_frame_creation() {
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(temp.path(), "失败导入测试").unwrap();
    let session = library::open_session(meta).unwrap();
    {
        let conn = session.db.lock().unwrap();
        let id = clip_studio_lib::db::assets::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: None,
                file_name: "photo.jpg",
                rel_path: "images/photo.jpg",
                file_hash: "test-hash",
                file_size: 100,
                mime_type: "image/jpeg",
                kind: "image",
                imported_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET import_stage = 'error:ffmpeg missing' WHERE id = ?1",
            [id],
        )
        .unwrap();
    }

    let error = preflight(&session).unwrap_err();
    assert_eq!(error.code(), "vector_rebuild_no_frames");
}

#[test]
fn corrupt_index_opens_library_in_rebuild_pending_state() {
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(temp.path(), "损坏索引测试").unwrap();
    drop(library::open_session(meta.clone()).unwrap());

    fs::write(
        temp.path().join(".clipstudio-open/vector/edge_config.json"),
        b"not-json",
    )
    .unwrap();

    let session = library::open_session(meta).unwrap();
    let health = session.vector_health.lock().unwrap().clone();
    assert!(matches!(health, VectorHealth::RebuildPending { .. }));
    assert!(session.vector.lock().unwrap().is_none());
}

#[test]
fn invalid_rebuild_journal_does_not_block_database_access() {
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(temp.path(), "恢复日志损坏测试").unwrap();
    drop(library::open_session(meta.clone()).unwrap());
    fs::write(
        temp.path().join(".clipstudio-open/vector-rebuild.json"),
        b"not-json",
    )
    .unwrap();

    let session = library::open_session(meta)
        .expect("A corrupt index journal must not block the library database");
    assert!(matches!(
        *session.vector_health.lock().unwrap(),
        VectorHealth::RebuildPending { .. }
    ));
    let count: i64 = session
        .db
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM assets", [], |row| row.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn interrupted_swap_restores_backup_without_starting_a_rebuild() {
    let temp = tempfile::tempdir().unwrap();
    let app_dir = temp.path().join(".clipstudio-open");
    let backup = app_dir.join("vector.backup-test");
    let staging = app_dir.join("vector.rebuilding-test");
    fs::create_dir_all(&backup).unwrap();
    fs::create_dir_all(&staging).unwrap();
    fs::write(backup.join("old.marker"), b"old").unwrap();
    fs::write(staging.join("new.marker"), b"new").unwrap();
    let journal = RebuildJournal {
        rebuild_id: "test".into(),
        phase: RebuildPhase::OldIndexMoved,
        staging_dir: "vector.rebuilding-test".into(),
        backup_dir: "vector.backup-test".into(),
        updated_at: 1,
    };
    fs::write(
        app_dir.join("vector-rebuild.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();

    let outcome = recover_interrupted_swap(temp.path()).unwrap();

    assert!(outcome.restored_backup);
    assert!(app_dir.join("vector/old.marker").is_file());
    assert!(
        staging.is_dir(),
        "Recovery must neither resume nor delete an unfinished rebuild"
    );
}

#[test]
fn uncommitted_new_directory_is_rolled_back_to_verified_old_index() {
    let temp = tempfile::tempdir().unwrap();
    let app_dir = temp.path().join(".clipstudio-open");
    let current = app_dir.join("vector");
    let backup = app_dir.join("vector.backup-test");
    fs::create_dir_all(&current).unwrap();
    fs::create_dir_all(&backup).unwrap();
    fs::write(current.join("new.marker"), b"new").unwrap();
    fs::write(backup.join("old.marker"), b"old").unwrap();
    let journal = RebuildJournal {
        rebuild_id: "test".into(),
        phase: RebuildPhase::OldIndexMoved,
        staging_dir: "vector.rebuilding-test".into(),
        backup_dir: "vector.backup-test".into(),
        updated_at: 1,
    };
    fs::write(
        app_dir.join("vector-rebuild.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();

    let outcome = recover_interrupted_swap(temp.path()).unwrap();

    assert!(outcome.restored_backup);
    assert!(current.join("old.marker").is_file());
    assert!(app_dir.join("vector.rebuilding-test/new.marker").is_file());
}

#[test]
fn rebuild_journal_directory_names_cannot_escape_library_data() {
    let temp = tempfile::tempdir().unwrap();
    let app_dir = temp.path().join(".clipstudio-open");
    fs::create_dir_all(&app_dir).unwrap();
    let journal = RebuildJournal {
        rebuild_id: "test".into(),
        phase: RebuildPhase::OldIndexMoved,
        staging_dir: "../../outside".into(),
        backup_dir: "vector.backup-test".into(),
        updated_at: 1,
    };
    fs::write(
        app_dir.join("vector-rebuild.json"),
        serde_json::to_vec(&journal).unwrap(),
    )
    .unwrap();

    assert!(recover_interrupted_swap(temp.path()).is_err());
    assert!(!temp.path().join("outside").exists());
}
