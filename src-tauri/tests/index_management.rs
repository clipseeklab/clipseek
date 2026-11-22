//! Resource-independent compatibility and indexing contracts.
use clip_studio_lib::{db, library};
use rusqlite::Connection;

#[test]
fn newer_database_is_rejected_without_overwriting_its_version_or_journal() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("future.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL); INSERT INTO meta VALUES('schema_version','999'); CREATE TABLE future_data(value TEXT); INSERT INTO future_data VALUES('keep');").unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    let result = db::open_library_db(&path);
    assert!(result.is_err(), "must refuse a newer schema");
    assert_eq!(result.err().unwrap().code(), "library_upgrade_required");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        db::open_library_read_db(&path).err().unwrap().code(),
        "library_upgrade_required"
    );
}

#[test]
fn opening_a_future_library_does_not_run_vector_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(temp.path(), "Future").unwrap();
    let path = temp.path().join(".clipstudio-open/library.db");
    let conn = Connection::open(path).unwrap();
    conn.execute("UPDATE meta SET value='999' WHERE key='schema_version'", [])
        .unwrap();
    drop(conn);
    let marker = temp.path().join(".clipstudio-open/do-not-touch");
    std::fs::write(&marker, "keep").unwrap();
    assert_eq!(
        library::open_session(meta.clone()).err().unwrap().code(),
        "library_upgrade_required"
    );
    assert_eq!(
        library::open_session_read_only(meta).err().unwrap().code(),
        "library_upgrade_required"
    );
    assert_eq!(std::fs::read_to_string(marker).unwrap(), "keep");
}

fn setup() -> (tempfile::TempDir, std::sync::Arc<library::LibrarySession>) {
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(temp.path(), "Indexes").unwrap();
    let session = std::sync::Arc::new(library::open_session(meta).unwrap());
    (temp, session)
}
fn insert(session: &library::LibrarySession, name: &str, kind: &str) -> i64 {
    db::assets::insert_asset(
        &session.db.lock().unwrap(),
        clip_studio_lib::db::assets::NewAsset {
            folder_id: None,
            file_name: name,
            rel_path: name,
            file_hash: name,
            file_size: 1,
            mime_type: "video/mp4",
            kind,
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn light_import_finishes_without_frames_or_models_and_snapshots_policy() {
    use clip_studio_lib::import;
    let (_temp, session) = setup();
    let source = tempfile::tempdir().unwrap();
    let path = source.path().join("image.svg");
    std::fs::write(&path,r##"<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><rect width="16" height="16" fill="#336699"/></svg>"##).unwrap();
    let policy = db::indexing::IndexPolicy {
        visual: false,
        subtitles: false,
    };
    let summary = import::run_import_with_policy(
        &session,
        &[path],
        None,
        &std::sync::atomic::AtomicBool::new(false),
        import::ImportMode::Reference,
        policy,
        clip_studio_lib::import::ImportCallbacks {
            progress: &|_, _, _| {},
            on_asset: &|_| {},
        },
    );
    assert_eq!(summary.done, 1);
    assert_eq!(summary.failed, 0);
    let conn = session.read_db();
    let asset = db::assets::query_assets(
        &conn,
        &session.root,
        &db::model::AssetFilter {
            page: 1,
            page_size: 10,
            ..Default::default()
        },
    )
    .unwrap()
    .items
    .remove(0);
    assert_eq!(asset.import_stage, "done");
    assert!(!asset.visual_indexed);
    assert!(std::path::Path::new(&asset.thumb_path.unwrap()).is_file());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM frames", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(db::indexing::tasks(&conn).unwrap().is_empty());
    assert!(!db::indexing::policy(&conn, asset.id).unwrap().visual);
}

#[test]
fn queued_scopes_include_all_pages_deduplicate_and_skip_indexed_or_deleted_assets() {
    let (_temp, session) = setup();
    let mut conn = session.db.lock().unwrap();
    let folder = clip_studio_lib::db::folders::insert(&conn, "Batch", 1).unwrap();
    conn.execute("INSERT INTO tags(name,created_at) VALUES('Tag',1)", [])
        .unwrap();
    let tag = conn.last_insert_rowid();
    for i in 0..245 {
        let id = db::assets::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: Some(folder),
                file_name: &format!("{i}.png"),
                rel_path: &format!("{i}.png"),
                file_hash: &format!("hash{i}"),
                file_size: 1,
                mime_type: "image/png",
                kind: "image",
                imported_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "INSERT INTO asset_tags(asset_id,tag_id) VALUES(?1,?2)",
            rusqlite::params![id, tag],
        )
        .unwrap();
    }
    conn.execute("UPDATE assets SET visual_indexed=1 WHERE id=1", [])
        .unwrap();
    conn.execute("UPDATE assets SET deleted_at=1 WHERE id=2", [])
        .unwrap();
    let scope = db::indexing::IndexScope {
        folder_ids: vec![folder],
        tag_ids: vec![tag],
        ..Default::default()
    };
    let tx = conn.transaction().unwrap();
    assert_eq!(
        db::indexing::enqueue_scope(&tx, &scope, &["visual".into()]).unwrap(),
        243
    );
    assert_eq!(
        db::indexing::enqueue_scope(&tx, &scope, &["visual".into()]).unwrap(),
        0
    );
    tx.commit().unwrap();
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM index_tasks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        243
    );
}

#[test]
fn index_filters_can_find_unindexed_assets_without_vectors() {
    let (_temp, session) = setup();
    let first = insert(&session, "a.png", "image");
    let second = insert(&session, "b.png", "image");
    session
        .db
        .lock()
        .unwrap()
        .execute("UPDATE assets SET visual_indexed=1 WHERE id=?", [first])
        .unwrap();
    let rows = db::assets::query_assets(
        &session.read_db(),
        &session.root,
        &db::model::AssetFilter {
            visual_indexed: Some(false),
            page: 1,
            page_size: 10,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(rows.total, 1);
    assert_eq!(rows.items[0].id, second);
}

#[test]
fn migrating_v18_backs_up_and_preserves_completed_indexes_and_import_policy() {
    let (temp, session) = setup();
    let id = insert(&session, "legacy.png", "image");
    {
        let conn = session.db.lock().unwrap();
        conn.execute("UPDATE assets SET import_stage='done' WHERE id=?", [id])
            .unwrap();
        let frame = db::assets::insert_frame(&conn, id, 0, 0, None).unwrap();
        db::assets::mark_frame_embedded(&conn, frame).unwrap();
        conn.execute("UPDATE assets SET visual_indexed=0 WHERE id=?", [id])
            .unwrap();
        conn.execute_batch("DROP TRIGGER processing_asset_trash; DROP TRIGGER processing_asset_delete;
            DROP TABLE processing_items; DROP TABLE processing_history; DROP TABLE processing_tasks; DROP TABLE processing_state;
            ALTER TABLE index_tasks DROP COLUMN origin; ALTER TABLE index_tasks DROP COLUMN batch_id;").unwrap();
        conn.execute("DROP TRIGGER schema_version_no_downgrade", [])
            .unwrap();
        conn.execute("UPDATE meta SET value='18' WHERE key='schema_version'", [])
            .unwrap();
        db::import_jobs::start(&conn, "snapshot", "paths", &["a.png".into()], None).unwrap();
        conn.execute(
            "UPDATE import_jobs SET visual_index=0,subtitle_index=1 WHERE id='snapshot'",
            [],
        )
        .unwrap();
    }
    let meta = session.meta.clone();
    drop(session);
    let reopened = library::open_session_light(meta).unwrap();
    assert!(
        db::assets::get_asset(&reopened.read_db(), &reopened.root, id)
            .unwrap()
            .unwrap()
            .visual_indexed
    );
    let job = db::import_jobs::get(&reopened.read_db(), "snapshot")
        .unwrap()
        .unwrap();
    assert!(!job.index_policy.visual);
    assert!(job.index_policy.subtitles);
    let backups = std::fs::read_dir(temp.path().join(".clipstudio-open/backups"))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(backups.len(), 1);
    let snapshot = Connection::open(backups[0].path()).unwrap();
    assert_eq!(
        snapshot
            .query_row(
                "SELECT value FROM meta WHERE key='schema_version'",
                [],
                |r| r.get::<_, String>(0)
            )
            .unwrap(),
        "18"
    );
}

#[test]
fn interrupted_index_tasks_are_requeued_without_enabling_disabled_auto_indexing() {
    let (_temp, session) = setup();
    let id = insert(&session, "movie.mp4", "video");
    {
        let conn = session.db.lock().unwrap();
        db::indexing::set_policy(
            &conn,
            id,
            db::indexing::IndexPolicy {
                visual: false,
                subtitles: false,
            },
        )
        .unwrap();
        db::indexing::enqueue(&conn, id, "subtitles").unwrap();
        clip_studio_lib::db::indexing::set_status(
            &conn,
            None,
            "running",
            None,
            &["queued", "running"],
        )
        .unwrap();
    }
    let meta = session.meta.clone();
    drop(session);
    let reopened = library::open_session_light(meta).unwrap();
    assert_eq!(
        db::indexing::tasks(&reopened.read_db()).unwrap()[0].status,
        "queued"
    );
    assert!(
        !db::indexing::policy(&reopened.read_db(), id)
            .unwrap()
            .visual
    );
}

#[test]
fn newer_library_metadata_is_rejected_before_typed_parsing_or_rewriting() {
    let (_temp, session) = setup();
    let meta = session.meta.clone();
    let root = session.root.clone();
    drop(session);
    let path = root.join(library::LIBRARY_CONFIG_FILE);
    let bytes = b"{\"version\":999,\"futureLayout\":true}";
    std::fs::write(&path, bytes).unwrap();
    assert_eq!(
        library::open_session(meta.clone()).err().unwrap().code(),
        "library_upgrade_required"
    );
    assert_eq!(
        library::write_library_config(&root, &meta)
            .err()
            .unwrap()
            .code(),
        "library_upgrade_required"
    );
    let mut config = clip_studio_lib::config::GlobalConfig::default();
    assert_eq!(
        library::import_library(&mut config, root.to_str().unwrap())
            .err()
            .unwrap()
            .code(),
        "library_upgrade_required"
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn visual_state_and_subtitle_state_are_applied_before_top_k_recall() {
    use clip_studio_lib::{services::search::run_search_session, vector};
    let (_temp, session) = setup();
    let distractor = insert(&session, "unfinished.png", "image");
    let wanted = insert(&session, "wanted.mp4", "video");
    let subtitled = insert(&session, "subtitled.mp4", "video");
    let mut points = Vec::new();
    {
        let conn = session.db.lock().unwrap();
        conn.execute(
            "UPDATE assets SET visual_indexed=1 WHERE id IN (?1,?2)",
            rusqlite::params![wanted, subtitled],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET subtitle_indexed=1 WHERE id=?",
            [subtitled],
        )
        .unwrap();
        for (asset, count, score) in [
            (distractor, 1001, 1.0f32),
            (wanted, 1, 0.8f32),
            (subtitled, 1, 0.6f32),
        ] {
            for seq in 0..count {
                let frame = db::assets::insert_frame(&conn, asset, seq, 0, None).unwrap();
                let mut embedding = vec![0.0; 768];
                embedding[0] = score;
                embedding[1] = (1.0 - score * score).sqrt();
                points.push((
                    frame,
                    embedding,
                    serde_json::json!({"asset_id":asset,"deleted":false,"kind":"image"}),
                ));
            }
        }
    }
    vector::upsert_points(&session, points).unwrap();
    let mut query = vec![0.0; 768];
    query[0] = 1.0;
    let page = run_search_session(
        &session,
        query.clone(),
        Some(&db::model::AssetFilter {
            subtitle_indexed: Some(false),
            ..Default::default()
        }),
        None,
        None,
    )
    .unwrap();
    assert_eq!(
        page.total, 1,
        "unfinished high-scoring vectors must not exhaust top-k recall"
    );
    assert_eq!(page.items[0].asset.id, wanted);
    let page = run_search_session(
        &session,
        query.clone(),
        Some(&db::model::AssetFilter {
            subtitle_indexed: Some(true),
            ..Default::default()
        }),
        None,
        None,
    )
    .unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.items[0].asset.id, subtitled);
    let page = run_search_session(
        &session,
        query,
        Some(&db::model::AssetFilter {
            visual_indexed: Some(false),
            ..Default::default()
        }),
        None,
        None,
    )
    .unwrap();
    assert_eq!(page.total, 0);
}

#[test]
fn database_guard_blocks_the_exact_legacy_version_upsert_with_an_upgrade_message() {
    let (_temp, session) = setup();
    let conn = session.db.lock().unwrap();
    let error=conn.execute("INSERT INTO meta(key,value) VALUES('schema_version',18) ON CONFLICT(key) DO UPDATE SET value=excluded.value",[]).unwrap_err();
    assert!(error.to_string().contains("upgrade ClipSeek Open"));
    assert_eq!(
        conn.query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        db::SCHEMA_VERSION
    );
}

#[test]
fn trash_cancels_pending_index_work_and_shared_repair_excludes_cancelled_generations() {
    let (_temp, session) = setup();
    let cancelled = insert(&session, "cancelled.png", "image");
    let valid = insert(&session, "valid.png", "image");
    {
        let conn = session.db.lock().unwrap();
        db::assets::insert_frame(&conn, cancelled, 0, 0, None).unwrap();
        db::indexing::enqueue(&conn, cancelled, "visual").unwrap();
        db::assets::set_deleted(&conn, &[cancelled], Some(1)).unwrap();
        assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "cancelled");
        db::assets::set_deleted(&conn, &[cancelled], None).unwrap();
        let frame = db::assets::insert_frame(&conn, valid, 0, 0, None).unwrap();
        db::assets::mark_frame_embedded(&conn, frame).unwrap();
        let rows = db::assets::frames_full_after(&conn, &session.root, 0, 200).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].asset_id, valid);
    }
    assert_eq!(
        clip_studio_lib::vector::recovery::preflight(&session)
            .unwrap()
            .frame_count,
        1
    );
}

#[test]
fn vector_reconciliation_ignores_cancelled_partial_indexes() {
    let (_temp, session) = setup();
    let id = insert(&session, "partial.png", "image");
    {
        let conn = session.db.lock().unwrap();
        let frame = db::assets::insert_frame(&conn, id, 0, 0, None).unwrap();
        conn.execute("UPDATE frames SET embedded=1 WHERE id=?", [frame])
            .unwrap();
        db::indexing::enqueue(&conn, id, "visual").unwrap();
        clip_studio_lib::db::indexing::set_status(
            &conn,
            None,
            "cancelled",
            None,
            &["queued", "running"],
        )
        .unwrap();
    }
    assert_eq!(
        clip_studio_lib::vector::reconcile_embedding_state(&session).unwrap(),
        0
    );
    assert_eq!(
        db::assets::count_pending_frames(&session.read_db()).unwrap(),
        0
    );
    assert_eq!(
        db::indexing::tasks(&session.read_db()).unwrap()[0].status,
        "cancelled"
    );
}

#[test]
fn invalid_index_requests_leave_the_queue_unchanged() {
    let (_temp, session) = setup();
    let id = insert(&session, "movie.mp4", "video");
    let scope = db::indexing::IndexScope {
        asset_ids: vec![id],
        ..Default::default()
    };
    let conn = session.db.lock().unwrap();
    for kinds in [
        vec![],
        vec!["unknown".into()],
        vec!["visual".into(), "invalid".into()],
    ] {
        assert!(db::indexing::enqueue_scope(&conn, &scope, &kinds).is_err());
        assert!(db::indexing::tasks(&conn).unwrap().is_empty());
    }
    assert!(db::indexing::enqueue_scope(&conn, &Default::default(), &["visual".into()]).is_err());
    assert!(db::indexing::tasks(&conn).unwrap().is_empty());
}

#[test]
fn active_tasks_deduplicate_and_terminal_tasks_allow_a_fresh_attempt() {
    let (_temp, session) = setup();
    let id = insert(&session, "movie.mp4", "video");
    let conn = session.db.lock().unwrap();
    assert_eq!(db::indexing::enqueue(&conn, id, "visual").unwrap(), 1);
    assert_eq!(db::indexing::enqueue(&conn, id, "visual").unwrap(), 0);
    clip_studio_lib::db::indexing::set_status(&conn, None, "running", None, &["queued", "running"])
        .unwrap();
    assert_eq!(db::indexing::enqueue(&conn, id, "visual").unwrap(), 0);
    for status in ["failed", "cancelled", "no_source"] {
        clip_studio_lib::db::indexing::set_status(
            &conn,
            None,
            status,
            None,
            &["queued", "running"],
        )
        .unwrap();
        assert_eq!(
            db::indexing::enqueue(&conn, id, "visual").unwrap(),
            1,
            "{status}"
        );
    }
    let tasks = db::indexing::tasks_for_asset(&conn, Some(id)).unwrap();
    assert_eq!(tasks.len(), 4);
    assert_eq!(tasks.iter().filter(|t| t.status == "queued").count(), 1);
    assert_eq!(db::indexing::enqueue(&conn, id + 999, "visual").unwrap(), 0);
}

#[test]
fn even_an_explicit_trash_filter_cannot_enqueue_deleted_assets() {
    let (_temp, session) = setup();
    let live = insert(&session, "live.mp4", "video");
    let deleted = insert(&session, "deleted.mp4", "video");
    let conn = session.db.lock().unwrap();
    conn.execute("UPDATE assets SET deleted_at=1 WHERE id=?", [deleted])
        .unwrap();
    let scope = db::indexing::IndexScope {
        all: true,
        filter: Some(db::model::AssetFilter {
            in_trash: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    db::indexing::enqueue_scope(&conn, &scope, &["visual".into()]).unwrap();
    assert!(!db::indexing::tasks(&conn)
        .unwrap()
        .iter()
        .any(|t| t.asset_id == deleted));
    assert_eq!(db::indexing::enqueue(&conn, deleted, "visual").unwrap(), 0);
    assert_eq!(db::indexing::enqueue(&conn, live, "visual").unwrap(), 1);
}

#[test]
fn changed_reference_invalidates_only_its_recorded_index_kind() {
    let (temp, session) = setup();
    let reference = insert(&session, "reference.mp4", "video");
    let managed = insert(&session, "managed.mp4", "video");
    let path = temp.path().join("reference.mp4");
    std::fs::write(&path, b"first").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let mtime = meta
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
        .to_string();
    let conn = session.db.lock().unwrap();
    conn.execute(
        "UPDATE assets SET source_path=?1 WHERE id=?2",
        rusqlite::params![path.to_str().unwrap(), reference],
    )
    .unwrap();
    conn.execute("UPDATE assets SET visual_indexed=1,subtitle_indexed=1", [])
        .unwrap();
    for id in [reference, managed] {
        conn.execute(
            "INSERT INTO index_versions VALUES(?,'visual','1',?,?)",
            rusqlite::params![id, meta.len() as i64, mtime],
        )
        .unwrap();
    }
    db::indexing::invalidate_changed_references(&conn).unwrap();
    assert!(
        db::assets::get_asset(&conn, &session.root, reference)
            .unwrap()
            .unwrap()
            .visual_indexed
    );
    std::fs::write(&path, b"a changed source with a different size").unwrap();
    db::indexing::invalidate_changed_references(&conn).unwrap();
    let asset = db::assets::get_asset(&conn, &session.root, reference)
        .unwrap()
        .unwrap();
    assert!(!asset.visual_indexed);
    assert!(asset.subtitle_indexed);
    assert!(
        db::assets::get_asset(&conn, &session.root, managed)
            .unwrap()
            .unwrap()
            .visual_indexed
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        b"a changed source with a different size"
    );
}

#[test]
fn deleting_an_asset_cascades_index_tasks_versions_and_saved_subtitle_bytes() {
    let (_temp, session) = setup();
    let deleted = insert(&session, "deleted.mp4", "video");
    let kept = insert(&session, "kept.mp4", "video");
    let conn = session.db.lock().unwrap();
    for id in [deleted, kept] {
        db::indexing::enqueue(&conn, id, "visual").unwrap();
        conn.execute(
            "INSERT INTO index_versions VALUES(?,'visual','1',1,'0')",
            [id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO subtitle_sources VALUES(?,'movie.srt',X'01')",
            [id],
        )
        .unwrap();
    }
    conn.execute("DELETE FROM assets WHERE id=?", [deleted])
        .unwrap();
    for table in ["index_tasks", "index_versions", "subtitle_sources"] {
        let ids: Vec<i64> = conn
            .prepare(&format!("SELECT asset_id FROM {table}"))
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(ids, vec![kept], "{table}");
    }
}

#[test]
fn automatic_index_and_retry_keep_the_registered_source_path() {
    let (_temp, session) = setup();
    let asset = insert(&session, "managed.png", "image");
    let conn = session.db.lock().unwrap();
    let source = "/external/original.png";
    let record =
        db::processing::register(&conn, "paths", std::path::Path::new(source), None).unwrap();
    db::processing::registered(&conn, record, Some(asset), "completed", None).unwrap();
    db::indexing::enqueue(&conn, asset, "visual").unwrap();
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(
        page.items[0].source_path, source,
        "automatic indexing must keep the original import source"
    );
    db::processing::retry_import(&conn, asset).unwrap();
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(
        page.items[0].source_path, source,
        "an import retry must keep its original source metadata"
    );
}

#[test]
fn retrying_deleted_unfinished_import_preserves_one_queue_identity() {
    let (_temp, session) = setup();
    let asset = insert(&session, "deleted.png", "image");
    let conn = session.db.lock().unwrap();
    let path = std::path::Path::new("/external/deleted.png");
    let old = db::processing::register(&conn, "deleted", path, None).unwrap();
    db::processing::registered(&conn, old, Some(asset), "running", None).unwrap();
    conn.execute("UPDATE assets SET deleted_at=1 WHERE id=?", [asset])
        .unwrap();
    conn.execute("DELETE FROM assets WHERE id=?", [asset])
        .unwrap();
    let retry = db::processing::register(&conn, "deleted", path, None).unwrap();
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(
        page.total, 1,
        "retrying the same registered item must not leave a duplicate cancelled material"
    );
    assert_eq!(page.items[0].tasks[0].id, retry);
    assert_eq!(page.items[0].tasks[0].attempt, 2);
    let history = db::processing::history(&conn, &page.items[0].key, None).unwrap();
    assert_eq!(history.tasks.len(), 2);
    assert_eq!(history.tasks[1].id, old);
    assert_eq!(history.tasks[1].status, "cancelled");
}

#[test]
fn visual_kinds_are_deduplicated_and_subtitle_scheduling_is_rejected() {
    let (_temp, s) = setup();
    insert(&s, "photo.png", "image");
    insert(&s, "video.mp4", "video");
    let c = s.db.lock().unwrap();
    let scope = db::indexing::IndexScope {
        all: true,
        ..Default::default()
    };
    assert!(db::indexing::enqueue_scope(&c, &scope, &["subtitles".into()]).is_err());
    assert_eq!(
        db::indexing::enqueue_scope(&c, &scope, &["visual".into(), "visual".into()]).unwrap(),
        2
    );
    assert_eq!(
        db::indexing::enqueue_scope(&c, &scope, &["visual".into()]).unwrap(),
        0
    );
}
