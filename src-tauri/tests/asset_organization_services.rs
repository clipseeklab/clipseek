//! Model-free behavior contracts for shared asset organization and lifecycle operations.

use std::fs;

use clip_studio_lib::db::{self, model::Asset, model::AssetFilter};
use clip_studio_lib::library::{self, LibraryDirs, LibrarySession};
use clip_studio_lib::services::assets::{
    delete_permanent, move_assets_to_folder, restore_assets, trash_assets,
};
use clip_studio_lib::{services, vector};
use serde_json::json;

struct Fixture {
    session: LibrarySession,
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let meta = library::init_library(&temp.path().join("library"), "Organization").unwrap();
        Self {
            session: library::open_session(meta).unwrap(),
            temp,
        }
    }

    fn folder(&self) -> i64 {
        let conn = self.session.db.lock().unwrap();
        db::folders::insert(&conn, "Target", 1).unwrap()
    }

    fn asset(&self, name: &str, reference: bool) -> Asset {
        let uid = uuid::Uuid::now_v7().to_string();
        let dir = LibraryDirs::new(&self.session.root).asset(1, &uid);
        fs::create_dir_all(&dir).unwrap();
        let source = if reference {
            self.temp.path().join(name)
        } else {
            dir.join(name)
        };
        fs::write(&source, b"original content").unwrap();
        let rel = dir
            .join(name)
            .strip_prefix(&self.session.root)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let conn = self.session.db.lock().unwrap();
        let id = db::assets::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: None,
                file_name: name,
                rel_path: &rel,
                file_hash: &uid,
                file_size: 16,
                mime_type: "image/jpeg",
                kind: "image",
                imported_at: 1,
                uid: Some(&uid),
                ..Default::default()
            },
        )
        .unwrap();
        if reference {
            conn.execute(
                "UPDATE assets SET source_path=?1 WHERE id=?2",
                rusqlite::params![source.to_string_lossy(), id],
            )
            .unwrap();
        }
        db::assets::get_asset(&conn, &self.session.root, id)
            .unwrap()
            .unwrap()
    }

    fn current(&self, id: i64) -> Option<Asset> {
        db::assets::get_asset(&self.session.read_db(), &self.session.root, id).unwrap()
    }
}

#[test]
fn moves_multiple_assets_without_touching_managed_or_reference_files() {
    let fixture = Fixture::new();
    let managed = fixture.asset("managed.jpg", false);
    let reference = fixture.asset("external.jpg", true);
    let target = fixture.folder();
    let ids = [managed.id, reference.id];
    move_assets_to_folder(&fixture.session, &ids, Some(target)).unwrap();
    for original in [&managed, &reference] {
        let moved = fixture.current(original.id).unwrap();
        assert_eq!(moved.folder_id, Some(target));
        assert_eq!(moved.abs_path, original.abs_path);
        assert_eq!(moved.rel_path, original.rel_path);
        assert_eq!(fs::read(&moved.abs_path).unwrap(), b"original content");
    }
    move_assets_to_folder(&fixture.session, &ids, None).unwrap();
    assert!(ids
        .iter()
        .all(|id| fixture.current(*id).unwrap().folder_id.is_none()));
}

#[test]
fn missing_target_folder_rejects_the_batch_before_any_asset_changes() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let error = move_assets_to_folder(&fixture.session, &[original.id], Some(999)).unwrap_err();
    assert_eq!(error.code(), "folder_not_found");
    assert_eq!(
        fixture.current(original.id).unwrap().folder_id,
        original.folder_id
    );
    assert_eq!(fs::read(original.abs_path).unwrap(), b"original content");
}

#[test]
fn uid_entry_validates_the_entire_selection_before_mutating() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let target = fixture.folder();
    let error = services::move_assets_to_folder(
        &fixture.session,
        &[original.uid.clone(), "unknown".into()],
        Some(target),
    )
    .unwrap_err();
    assert_eq!(error.code(), "asset_not_found");
    assert!(fixture.current(original.id).unwrap().folder_id.is_none());
    for selection in [vec![], vec![original.uid; 101]] {
        assert_eq!(
            services::move_assets_to_folder(&fixture.session, &selection, Some(target))
                .unwrap_err()
                .code(),
            "invalid_argument"
        );
    }
}

#[test]
fn folder_changes_update_search_filters_without_rebuilding_vectors() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let target = fixture.folder();
    let frame_id =
        db::assets::insert_frame(&fixture.session.db.lock().unwrap(), original.id, 0, 0, None)
            .unwrap();
    let embedding = vec![1.0_f32; 768];
    vector::upsert_points(
        &fixture.session,
        vec![(
            frame_id,
            embedding.clone(),
            json!({"asset_id":original.id,"folder_id":null,"deleted":false}),
        )],
    )
    .unwrap();
    let filter = vector::build_filter(&AssetFilter {
        folder_id: Some(target),
        ..Default::default()
    });
    assert!(
        vector::search(&fixture.session, embedding.clone(), filter.clone(), 10)
            .unwrap()
            .is_empty()
    );
    move_assets_to_folder(&fixture.session, &[original.id], Some(target)).unwrap();
    let hits = vector::search(&fixture.session, embedding.clone(), filter.clone(), 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(
        hits[0].payload.as_ref().unwrap().0.get("folder_id"),
        Some(&json!(target))
    );
    move_assets_to_folder(&fixture.session, &[original.id], None).unwrap();
    assert!(vector::search(&fixture.session, embedding, filter, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn managed_assets_round_trip_through_trash_and_are_permanently_removed() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let target = fixture.folder();
    move_assets_to_folder(&fixture.session, &[original.id], Some(target)).unwrap();
    trash_assets(&fixture.session, &[original.id]).unwrap();
    let trashed = fixture.current(original.id).unwrap();
    assert!(trashed.deleted_at.is_some());
    assert!(!std::path::Path::new(&original.abs_path).exists());
    assert_eq!(fs::read(&trashed.abs_path).unwrap(), b"original content");
    restore_assets(&fixture.session, &[original.id]).unwrap();
    let restored = fixture.current(original.id).unwrap();
    assert!(restored.deleted_at.is_none());
    assert_eq!(restored.folder_id, Some(target));
    assert_eq!(fs::read(&restored.abs_path).unwrap(), b"original content");
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).is_none());
    assert!(!std::path::Path::new(&restored.abs_path).exists());
}

#[test]
fn reference_source_survives_trash_restore_and_permanent_deletion() {
    let fixture = Fixture::new();
    let original = fixture.asset("external.jpg", true);
    trash_assets(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).unwrap().deleted_at.is_some());
    assert_eq!(fs::read(&original.abs_path).unwrap(), b"original content");
    restore_assets(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).unwrap().deleted_at.is_none());
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).is_none());
    assert_eq!(fs::read(original.abs_path).unwrap(), b"original content");
}

#[test]
fn failed_folder_batch_rolls_back_earlier_assets_and_can_be_retried() {
    let fixture = Fixture::new();
    let first = fixture.asset("first.jpg", false);
    let second = fixture.asset("second.jpg", true);
    let target = fixture.folder();
    {
        let conn = fixture.session.db.lock().unwrap();
        conn.execute_batch(&format!(
            "CREATE TRIGGER reject_folder_move BEFORE UPDATE OF folder_id ON assets
             WHEN NEW.id = {} BEGIN SELECT RAISE(ABORT, 'injected folder failure'); END;",
            second.id,
        ))
        .unwrap();
    }
    let ids = [first.id, second.id];
    assert!(move_assets_to_folder(&fixture.session, &ids, Some(target)).is_err());
    for asset in [&first, &second] {
        assert_eq!(
            fixture.current(asset.id).unwrap().folder_id,
            asset.folder_id
        );
        assert_eq!(fs::read(&asset.abs_path).unwrap(), b"original content");
    }
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_folder_move")
        .unwrap();
    move_assets_to_folder(&fixture.session, &ids, Some(target)).unwrap();
    for id in ids {
        assert_eq!(fixture.current(id).unwrap().folder_id, Some(target));
    }
}

fn reject_deleted_update(fixture: &Fixture, id: i64) {
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute_batch(&format!(
            "CREATE TRIGGER reject_lifecycle BEFORE UPDATE OF deleted_at ON assets
         WHEN NEW.id={id} BEGIN SELECT RAISE(ABORT, 'injected lifecycle failure'); END;"
        ))
        .unwrap();
}

fn allow_deleted_update(fixture: &Fixture) {
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_lifecycle")
        .unwrap();
}

#[test]
fn trash_database_failure_restores_the_entire_batch_and_allows_retry() {
    let fixture = Fixture::new();
    let first = fixture.asset("first.jpg", false);
    let second = fixture.asset("second.jpg", false);
    let reference = fixture.asset("external.jpg", true);
    reject_deleted_update(&fixture, second.id);
    let ids = [reference.id, first.id, second.id];
    assert!(trash_assets(&fixture.session, &ids).is_err());
    for original in [&first, &second, &reference] {
        let current = fixture.current(original.id).unwrap();
        assert_eq!(current.deleted_at, original.deleted_at);
        assert_eq!(current.rel_path, original.rel_path);
        assert_eq!(fs::read(&current.abs_path).unwrap(), b"original content");
    }
    allow_deleted_update(&fixture);
    trash_assets(&fixture.session, &ids).unwrap();
    trash_assets(&fixture.session, &ids).unwrap();
    assert!(ids
        .iter()
        .all(|id| fixture.current(*id).unwrap().deleted_at.is_some()));
    assert_eq!(fs::read(reference.abs_path).unwrap(), b"original content");
}

#[test]
fn restore_database_failure_keeps_trash_paths_and_preserves_existing_destination() {
    let fixture = Fixture::new();
    let first = fixture.asset("first.jpg", false);
    let second = fixture.asset("second.jpg", false);
    let reference = fixture.asset("external.jpg", true);
    let ids = [reference.id, first.id, second.id];
    trash_assets(&fixture.session, &ids).unwrap();
    let before: Vec<_> = ids.iter().map(|id| fixture.current(*id).unwrap()).collect();
    fs::write(&first.abs_path, b"existing destination").unwrap();
    reject_deleted_update(&fixture, second.id);
    assert!(restore_assets(&fixture.session, &ids).is_err());
    for original in &before {
        let current = fixture.current(original.id).unwrap();
        assert_eq!(current.deleted_at, original.deleted_at);
        assert_eq!(current.rel_path, original.rel_path);
        assert_eq!(fs::read(&current.abs_path).unwrap(), b"original content");
    }
    assert_eq!(fs::read(&first.abs_path).unwrap(), b"existing destination");
    allow_deleted_update(&fixture);
    restore_assets(&fixture.session, &ids).unwrap();
    restore_assets(&fixture.session, &ids).unwrap();
    assert!(ids
        .iter()
        .all(|id| fixture.current(*id).unwrap().deleted_at.is_none()));
    assert_ne!(fixture.current(first.id).unwrap().abs_path, first.abs_path);
    assert_eq!(fs::read(&first.abs_path).unwrap(), b"existing destination");
}

#[test]
fn missing_later_source_does_not_leave_earlier_assets_trashed() {
    let fixture = Fixture::new();
    let first = fixture.asset("first.jpg", false);
    let missing = fixture.asset("missing.jpg", false);
    fs::remove_file(&missing.abs_path).unwrap();
    assert!(trash_assets(&fixture.session, &[first.id, missing.id]).is_err());
    let current = fixture.current(first.id).unwrap();
    assert!(current.deleted_at.is_none());
    assert_eq!(current.rel_path, first.rel_path);
    assert_eq!(fs::read(current.abs_path).unwrap(), b"original content");
    fs::write(&missing.abs_path, b"recovered source").unwrap();
    trash_assets(&fixture.session, &[first.id, missing.id]).unwrap();
}

#[test]
fn trash_does_not_overwrite_an_existing_file_at_its_planned_destination() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let trash = LibraryDirs::new(&fixture.session.root).trash();
    fs::create_dir_all(&trash).unwrap();
    let occupied = trash.join(format!("{}_{}", original.id, original.file_name));
    fs::write(&occupied, b"existing trash content").unwrap();
    trash_assets(&fixture.session, &[original.id]).unwrap();
    assert_eq!(fs::read(&occupied).unwrap(), b"existing trash content");
    let current = fixture.current(original.id).unwrap();
    assert_ne!(std::path::Path::new(&current.abs_path), occupied);
    assert_eq!(fs::read(current.abs_path).unwrap(), b"original content");
}

#[test]
fn lifecycle_commit_failure_rolls_back_files_and_rows_for_trash_and_restore() {
    for restoring in [false, true] {
        let fixture = Fixture::new();
        let original = fixture.asset("managed.jpg", false);
        if restoring {
            trash_assets(&fixture.session, &[original.id]).unwrap();
        }
        let before = fixture.current(original.id).unwrap();
        fixture.session.db.lock().unwrap().execute_batch(
            "CREATE TABLE reject_commit (folder_id INTEGER REFERENCES folders(id) DEFERRABLE INITIALLY DEFERRED);
             CREATE TRIGGER reject_lifecycle_commit AFTER UPDATE OF deleted_at ON assets
             BEGIN INSERT INTO reject_commit VALUES(999999); END;"
        ).unwrap();
        let result = if restoring {
            restore_assets(&fixture.session, &[original.id])
        } else {
            trash_assets(&fixture.session, &[original.id])
        };
        assert!(result.is_err());
        let current = fixture.current(original.id).unwrap();
        assert_eq!(current.deleted_at, before.deleted_at);
        assert_eq!(current.rel_path, before.rel_path);
        assert_eq!(fs::read(current.abs_path).unwrap(), b"original content");
        fixture
            .session
            .db
            .lock()
            .unwrap()
            .execute_batch("DROP TRIGGER reject_lifecycle_commit")
            .unwrap();
        if restoring {
            restore_assets(&fixture.session, &[original.id]).unwrap();
        } else {
            trash_assets(&fixture.session, &[original.id]).unwrap();
        }
    }
}

#[test]
fn failed_trash_keeps_index_tasks_and_processing_revision_unchanged() {
    let fixture = Fixture::new();
    let first = fixture.asset("first.jpg", false);
    let second = fixture.asset("second.jpg", false);
    let revision: i64 = {
        let conn = fixture.session.db.lock().unwrap();
        db::indexing::enqueue(&conn, first.id, "visual").unwrap();
        conn.query_row(
            "SELECT revision FROM processing_state WHERE id=1",
            [],
            |row| row.get(0),
        )
        .unwrap()
    };
    reject_deleted_update(&fixture, second.id);
    assert!(trash_assets(&fixture.session, &[first.id, second.id]).is_err());
    let conn = fixture.session.read_db();
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "queued");
    assert_eq!(
        conn.query_row(
            "SELECT revision FROM processing_state WHERE id=1",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        revision
    );
}

#[test]
fn vector_failure_after_trash_commit_can_be_retried_without_moving_files_again() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let frame =
        db::assets::insert_frame(&fixture.session.db.lock().unwrap(), original.id, 0, 0, None)
            .unwrap();
    let embedding = vec![1.0_f32; 768];
    vector::upsert_points(
        &fixture.session,
        vec![(
            frame,
            embedding.clone(),
            json!({"asset_id": original.id, "deleted": false}),
        )],
    )
    .unwrap();
    *fixture.session.vector.lock().unwrap() = None;
    *fixture.session.vector_health.lock().unwrap() = library::VectorHealth::RebuildPending {
        code: "injected_failure".into(),
        message: "injected vector unavailability".into(),
    };
    assert!(trash_assets(&fixture.session, &[original.id]).is_err());
    let committed = fixture.current(original.id).unwrap();
    assert!(committed.deleted_at.is_some());
    assert_eq!(fs::read(&committed.abs_path).unwrap(), b"original content");
    *fixture.session.vector_health.lock().unwrap() = library::VectorHealth::Ready;
    trash_assets(&fixture.session, &[original.id]).unwrap();
    assert_eq!(
        fixture.current(original.id).unwrap().rel_path,
        committed.rel_path
    );
    let filter = vector::build_filter(&AssetFilter::default());
    assert!(
        vector::search(&fixture.session, embedding.clone(), filter.clone(), 10)
            .unwrap()
            .is_empty()
    );
    restore_assets(&fixture.session, &[original.id]).unwrap();
    assert_eq!(
        vector::search(&fixture.session, embedding, filter, 10)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn permanent_file_failure_is_reported_and_the_record_survives_for_retry() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    trash_assets(&fixture.session, &[original.id]).unwrap();
    let trashed = fixture.current(original.id).unwrap();
    // A directory where a file is expected causes a deterministic remove_file failure.
    fs::remove_file(&trashed.abs_path).unwrap();
    fs::create_dir(&trashed.abs_path).unwrap();
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    let pending = fixture
        .current(original.id)
        .expect("retain paths until cleanup succeeds");
    assert!(pending.deleted_at.is_some());
    fs::remove_dir(&trashed.abs_path).unwrap();
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).is_none());
    delete_permanent(&fixture.session, &[original.id]).unwrap();
}

#[test]
fn permanent_vector_failure_keeps_frame_identity_for_retry() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let frame =
        db::assets::insert_frame(&fixture.session.db.lock().unwrap(), original.id, 0, 0, None)
            .unwrap();
    let embedding = vec![1.0_f32; 768];
    vector::upsert_points(
        &fixture.session,
        vec![(
            frame,
            embedding.clone(),
            json!({"asset_id": original.id, "deleted": false}),
        )],
    )
    .unwrap();
    trash_assets(&fixture.session, &[original.id]).unwrap();
    *fixture.session.vector.lock().unwrap() = None;
    *fixture.session.vector_health.lock().unwrap() = library::VectorHealth::RebuildPending {
        code: "injected".into(),
        message: "injected".into(),
    };
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    assert!(fixture.current(original.id).is_some());
    assert_eq!(
        db::assets::get_frame_ids(&fixture.session.read_db(), original.id).unwrap()[0].0,
        frame
    );
    *fixture.session.vector_health.lock().unwrap() = library::VectorHealth::Ready;
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).is_none());
    assert!(vector::search(&fixture.session, embedding, None, 10)
        .unwrap()
        .is_empty());
}

#[test]
fn permanent_database_delete_failure_can_retry_after_files_are_already_gone() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    trash_assets(&fixture.session, &[original.id]).unwrap();
    let before = fixture.current(original.id).unwrap();
    fixture.session.db.lock().unwrap().execute_batch(
        "CREATE TRIGGER reject_permanent BEFORE DELETE ON assets BEGIN SELECT RAISE(ABORT, 'injected delete failure'); END;"
    ).unwrap();
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    assert!(
        !std::path::Path::new(&before.abs_path).exists(),
        "cleanup precedes final row deletion"
    );
    assert!(fixture.current(original.id).is_some());
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_permanent")
        .unwrap();
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).is_none());
}

#[test]
fn pending_cleanup_survives_reopen_blocks_restore_and_ignores_retention_delay() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    trash_assets(&fixture.session, &[original.id]).unwrap();
    let path = fixture.current(original.id).unwrap().abs_path;
    fs::remove_file(&path).unwrap();
    fs::create_dir(&path).unwrap();
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    assert!(db::asset_cleanup::pending(&fixture.session.read_db(), &original.uid).unwrap());
    let meta = fixture.session.meta.clone();
    drop(fixture.session);
    let session = library::open_session_light(meta).unwrap();
    assert_eq!(
        restore_assets(&session, &[original.id]).unwrap_err().code(),
        "asset_delete_pending"
    );
    fs::remove_dir(&path).unwrap();
    assert_eq!(
        clip_studio_lib::commands::assets::auto_purge_expired(&session, 365).unwrap(),
        1
    );
    assert!(
        db::assets::get_asset(&session.read_db(), &session.root, original.id)
            .unwrap()
            .is_none()
    );
    assert!(!db::asset_cleanup::pending(&session.read_db(), &original.uid).unwrap());
    // Keep the temporary library alive until the reopened session is released.
    drop(session);
    drop(fixture.temp);
}

#[test]
fn failed_cleanup_does_not_prevent_other_selected_assets_from_completing() {
    let fixture = Fixture::new();
    let blocked = fixture.asset("blocked.jpg", false);
    let ready = fixture.asset("ready.jpg", false);
    fs::remove_file(&blocked.abs_path).unwrap();
    fs::create_dir(&blocked.abs_path).unwrap();
    let error = delete_permanent(&fixture.session, &[blocked.id, ready.id]).unwrap_err();
    assert_eq!(error.code(), "asset_cleanup_failed");
    assert!(fixture.current(blocked.id).is_some());
    assert!(fixture.current(ready.id).is_none());
    assert!(!std::path::Path::new(&ready.abs_path).exists());
}

#[test]
fn permanent_reference_cleanup_rejects_a_thumbnail_path_that_aliases_the_user_source() {
    let fixture = Fixture::new();
    let original = fixture.asset("reference.jpg", true);
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET thumb_rel=?1 WHERE id=?2",
            rusqlite::params![original.abs_path, original.id],
        )
        .unwrap();
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    assert_eq!(fs::read(&original.abs_path).unwrap(), b"original content");
    assert!(fixture.current(original.id).is_some());
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET thumb_rel=NULL WHERE id=?1",
            [original.id],
        )
        .unwrap();
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert_eq!(fs::read(original.abs_path).unwrap(), b"original content");
}

#[test]
fn transfer_never_reuses_a_complete_target_with_pending_permanent_deletion() {
    let source = Fixture::new();
    let original = source.asset("managed.jpg", false);
    image::RgbImage::new(8, 8).save(&original.abs_path).unwrap();
    let original_bytes = fs::read(&original.abs_path).unwrap();
    source
        .session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET import_stage='done' WHERE id=?1",
            [original.id],
        )
        .unwrap();
    let target = Fixture::new();
    let copied = clip_studio_lib::transfer::copy_asset_to_session(
        &source.session,
        original.id,
        &target.session,
        None,
    )
    .unwrap();
    {
        let mut conn = target.session.db.lock().unwrap();
        let tx = conn.transaction().unwrap();
        db::asset_cleanup::mark(&tx, &original.uid).unwrap();
        db::assets::set_deleted(&tx, &[copied], Some(1)).unwrap();
        tx.commit().unwrap();
    }
    let error = clip_studio_lib::transfer::copy_asset_to_session(
        &source.session,
        original.id,
        &target.session,
        None,
    )
    .unwrap_err();
    assert_eq!(error.code(), "asset_delete_pending");
    assert_eq!(fs::read(original.abs_path).unwrap(), original_bytes);
    delete_permanent(&target.session, &[copied]).unwrap();
    clip_studio_lib::transfer::copy_asset_to_session(
        &source.session,
        original.id,
        &target.session,
        None,
    )
    .unwrap();
}

#[test]
fn permanent_derivative_failure_retains_intent_after_the_original_file_is_gone() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    trash_assets(&fixture.session, &[original.id]).unwrap();
    let path = fixture.current(original.id).unwrap().abs_path;
    let directory =
        LibraryDirs::new(&fixture.session.root).asset(original.imported_at, &original.uid);
    fs::remove_dir(&directory).unwrap();
    fs::write(&directory, b"unexpected file at derivative directory").unwrap();
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    assert!(!std::path::Path::new(&path).exists());
    assert!(fixture.current(original.id).is_some());
    assert!(db::asset_cleanup::pending(&fixture.session.read_db(), &original.uid).unwrap());
    fs::remove_file(directory).unwrap();
    delete_permanent(&fixture.session, &[original.id]).unwrap();
    assert!(fixture.current(original.id).is_none());
}

#[test]
fn pending_cleanup_is_excluded_from_search_even_when_vector_cleanup_cannot_start() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    let frame =
        db::assets::insert_frame(&fixture.session.db.lock().unwrap(), original.id, 0, 0, None)
            .unwrap();
    let embedding = vec![1.0_f32; 768];
    vector::upsert_points(
        &fixture.session,
        vec![(
            frame,
            embedding.clone(),
            json!({"asset_id": original.id, "deleted": false}),
        )],
    )
    .unwrap();
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET visual_indexed=1,thumb_rel=?1 WHERE id=?2",
            rusqlite::params![
                fixture.temp.path().join("outside.webp").to_string_lossy(),
                original.id
            ],
        )
        .unwrap();
    assert!(delete_permanent(&fixture.session, &[original.id]).is_err());
    let result =
        services::search::run_search_session(&fixture.session, embedding, None, None, None)
            .unwrap();
    assert_eq!(
        result.total, 0,
        "pending deletion must not depend on a stale vector payload"
    );
}

#[test]
fn media_processing_notifications_do_not_hold_the_database_mutex() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    image::RgbImage::new(8, 8).save(&original.abs_path).unwrap();
    let session = std::sync::Arc::new(fixture.session);
    clip_studio_lib::import::process_asset(
        &session,
        &LibraryDirs::new(&session.root),
        original.id,
        &|_| {
            assert!(
                session.db.try_lock().is_ok(),
                "observers must be able to read database state without deadlocking"
            );
        },
    )
    .unwrap();
}

#[test]
fn permanent_deletion_defers_cleanup_until_the_active_media_worker_releases_its_files() {
    let fixture = Fixture::new();
    let original = fixture.asset("managed.jpg", false);
    image::RgbImage::new(8, 8).save(&original.abs_path).unwrap();
    let session = std::sync::Arc::new(fixture.session);
    let (entered, ready) = std::sync::mpsc::channel();
    let (resume, released) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let worker_session = session.clone();
        let worker = scope.spawn(move || {
            clip_studio_lib::import::process_asset(
                &worker_session,
                &LibraryDirs::new(&worker_session.root),
                original.id,
                &|asset| {
                    if asset.import_stage == "probing" {
                        entered.send(()).unwrap();
                        released
                            .recv_timeout(std::time::Duration::from_secs(5))
                            .unwrap();
                    }
                },
            )
        });
        ready
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let deletion = delete_permanent(&session, &[original.id]);
        let source_preserved = std::path::Path::new(&original.abs_path).is_file();
        resume.send(()).unwrap();
        let processing = worker.join().unwrap();
        assert!(
            deletion.is_err(),
            "cleanup must defer while a worker can still write files"
        );
        assert!(
            source_preserved,
            "active worker source must not be removed underneath it"
        );
        assert!(
            processing.is_err(),
            "deletion intent cancels the worker at its next stage boundary"
        );
        assert!(db::asset_cleanup::pending(&session.read_db(), &original.uid).unwrap());
        delete_permanent(&session, &[original.id]).unwrap();
        assert!(
            db::assets::get_asset(&session.read_db(), &session.root, original.id)
                .unwrap()
                .is_none()
        );
        assert!(!LibraryDirs::new(&session.root)
            .asset(original.imported_at, &original.uid)
            .exists());
    });
}

#[test]
fn reopening_recovers_interrupted_trash_and_restore_before_exposing_the_session() {
    for restoring in [false, true] {
        let fixture = Fixture::new();
        let asset = fixture.asset("interrupted.jpg", false);
        if restoring {
            trash_assets(&fixture.session, &[asset.id]).unwrap();
        }
        let before = fixture.current(asset.id).unwrap();
        let source = std::path::Path::new(&before.abs_path);
        let target = if restoring {
            LibraryDirs::new(&fixture.session.root)
                .asset(asset.imported_at, &asset.uid)
                .join("restored.jpg")
        } else {
            LibraryDirs::new(&fixture.session.root)
                .trash()
                .join("interrupted.jpg")
        };
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let record = json!({"committed": false, "ids": [asset.id], "moves": [{
            "source": clip_studio_lib::util::to_rel(&fixture.session.root, source),
            "target": clip_studio_lib::util::to_rel(&fixture.session.root, &target),
            "hash": blake3::hash(&fs::read(source).unwrap()).to_hex().to_string()
        }]});
        fixture
            .session
            .db
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO meta(key,value) VALUES('asset_lifecycle.v1.interrupted',?1)",
                [record.to_string()],
            )
            .unwrap();
        // Simulate a process ending after the filesystem move but before SQLite commit.
        fs::rename(source, &target).unwrap();
        let meta = fixture.session.meta.clone();
        drop(fixture.session);
        let reopened = library::open_session(meta).unwrap();
        let current = db::assets::get_asset(&reopened.read_db(), &reopened.root, asset.id)
            .unwrap()
            .unwrap();
        assert_eq!(current.deleted_at, before.deleted_at);
        assert_eq!(fs::read(&current.abs_path).unwrap(), b"original content");
        assert!(!target.exists());
        assert_eq!(
            reopened
                .read_db()
                .query_row(
                    "SELECT COUNT(*) FROM meta WHERE key LIKE 'asset_lifecycle.v1.%'",
                    [],
                    |r| r.get::<_, i64>(0)
                )
                .unwrap(),
            0
        );
    }
}

#[test]
fn recovery_preserves_committed_files_but_refuses_conflicts_and_read_only_access() {
    for committed in [false, true] {
        let fixture = Fixture::new();
        let asset = fixture.asset("journal.jpg", false);
        let source = std::path::Path::new(&asset.abs_path);
        let target = LibraryDirs::new(&fixture.session.root)
            .trash()
            .join("journal.jpg");
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        let record = json!({"committed": committed, "ids": [asset.id], "moves": [{
            "source": clip_studio_lib::util::to_rel(&fixture.session.root, source),
            "target": clip_studio_lib::util::to_rel(&fixture.session.root, &target),
            "hash": blake3::hash(&fs::read(source).unwrap()).to_hex().to_string()
        }]});
        {
            let conn = fixture.session.db.lock().unwrap();
            conn.execute(
                "INSERT INTO meta(key,value) VALUES('asset_lifecycle.v1.recovery',?1)",
                [record.to_string()],
            )
            .unwrap();
            if committed {
                db::assets::set_asset_rel_path(
                    &conn,
                    asset.id,
                    &clip_studio_lib::util::to_rel(&fixture.session.root, &target),
                    &asset.file_name,
                )
                .unwrap();
                db::assets::set_deleted(&conn, &[asset.id], Some(2)).unwrap();
            }
        }
        fs::rename(source, &target).unwrap();
        if !committed {
            fs::write(source, b"external file").unwrap();
        }
        let meta = fixture.session.meta.clone();
        drop(fixture.session);
        assert!(library::open_session_read_only(meta.clone()).is_err());
        let reopened = library::open_session(meta.clone());
        if committed {
            let reopened = reopened.unwrap();
            assert!(!source.exists());
            assert_eq!(fs::read(&target).unwrap(), b"original content");
            assert!(
                db::assets::get_asset(&reopened.read_db(), &reopened.root, asset.id)
                    .unwrap()
                    .unwrap()
                    .deleted_at
                    .is_some()
            );
        } else {
            assert!(reopened.is_err());
            assert_eq!(fs::read(source).unwrap(), b"external file");
            assert_eq!(fs::read(&target).unwrap(), b"original content");
            // Resolving the collision makes the retained journal recoverable.
            fs::remove_file(source).unwrap();
            library::open_session(meta).unwrap();
            assert_eq!(fs::read(source).unwrap(), b"original content");
        }
    }
}

#[test]
fn committed_file_recovery_with_a_broken_vector_store_keeps_the_library_open_for_repair() {
    let fixture = Fixture::new();
    let asset = fixture.asset("vector-recovery.jpg", false);
    db::assets::insert_frame(&fixture.session.db.lock().unwrap(), asset.id, 0, 0, None).unwrap();
    let frame = db::assets::get_frame_ids(&fixture.session.read_db(), asset.id).unwrap()[0].0;
    vector::upsert_points(
        &fixture.session,
        vec![(
            frame,
            vec![1.0; 768],
            json!({"asset_id": asset.id, "deleted":false}),
        )],
    )
    .unwrap();
    *fixture.session.vector.lock().unwrap() = None;
    *fixture.session.vector_health.lock().unwrap() = library::VectorHealth::RebuildPending {
        code: "injected".into(),
        message: "injected".into(),
    };
    assert!(trash_assets(&fixture.session, &[asset.id]).is_err());
    let trashed = fixture.current(asset.id).unwrap();
    let meta = fixture.session.meta.clone();
    let vector_path = LibraryDirs::new(&fixture.session.root).vector();
    drop(fixture.session);
    let backup = vector_path.with_extension("backup-test");
    fs::rename(&vector_path, &backup).unwrap();
    fs::create_dir(&vector_path).unwrap();
    fs::write(
        vector_path.join("edge_config.json"),
        b"invalid configuration",
    )
    .unwrap();
    let reopened = library::open_session(meta)
        .expect("file recovery must not prevent opening the index repair UI");
    assert_eq!(fs::read(&trashed.abs_path).unwrap(), b"original content");
    let journal: String = reopened
        .read_db()
        .query_row(
            "SELECT value FROM meta WHERE key LIKE 'asset_lifecycle.v1.%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&journal).unwrap()["moves"],
        json!([])
    );
    fs::remove_dir_all(&vector_path).unwrap();
    fs::rename(&backup, &vector_path).unwrap();
    *reopened.vector_health.lock().unwrap() = library::VectorHealth::Ready;
    restore_assets(&reopened, &[asset.id]).unwrap();
    assert!(
        db::assets::get_asset(&reopened.read_db(), &reopened.root, asset.id)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none()
    );
}
