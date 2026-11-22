use clip_studio_lib::{db, import, library};
use std::sync::{atomic::AtomicBool, Arc};

fn setup() -> (tempfile::TempDir, Arc<library::LibrarySession>) {
    let dir = tempfile::tempdir().unwrap();
    let meta = library::init_library(dir.path(), "Queue").unwrap();
    let session = Arc::new(library::open_session(meta).unwrap());
    (dir, session)
}

#[test]
fn action_targets_preserve_the_source_and_index_identity_needed_for_retry() {
    let (_dir, session) = setup();
    let asset = insert(&session, "retry.png");
    let conn = session.db.lock().unwrap();
    let id = db::processing::record_attempt(
        &conn,
        db::processing::NewAttempt {
            item_key: "retry-item".into(),
            batch_id: "batch-one".into(),
            asset_id: Some(asset),
            file_name: "retry.png".into(),
            source_path: "/source/retry.png".into(),
            kind: "visual".into(),
            status: "failed".into(),
            index_task_id: Some(91),
            target_folder_id: Some(73),
            ..Default::default()
        },
    )
    .unwrap();
    let target = db::processing::get(&conn, id).unwrap().unwrap();
    assert_eq!(target.asset_id, Some(asset));
    assert_eq!(target.source_path, "/source/retry.png");
    assert_eq!(target.kind, "visual");
    assert_eq!(target.status, "failed");
    assert_eq!(target.index_task_id, Some(91));
    assert_eq!(target.batch_id, "batch-one");
    assert_eq!(target.target_folder_id, Some(73));
    assert!(db::processing::get(&conn, id + 1).unwrap().is_none());
}

#[test]
fn source_only_action_targets_keep_optional_links_absent() {
    let (_dir, session) = setup();
    let conn = session.db.lock().unwrap();
    let id = db::processing::register(
        &conn,
        "source-only",
        std::path::Path::new("/source/photo.png"),
        None,
    )
    .unwrap();
    let target = db::processing::get(&conn, id).unwrap().unwrap();
    assert!(target.asset_id.is_none());
    assert!(target.index_task_id.is_none());
    assert!(target.target_folder_id.is_none());
    assert_eq!(target.batch_id, "source-only");
    assert_eq!(target.source_path, "/source/photo.png");
    assert_eq!(target.kind, "import");
    assert_eq!(target.status, "queued");
}

#[test]
fn latest_summary_ignores_late_old_attempt_updates_and_rolls_back_atomically() {
    let (_dir, s) = setup();
    let mut conn = s.db.lock().unwrap();
    let first =
        db::processing::register(&conn, "retry", std::path::Path::new("/retry.png"), None).unwrap();
    db::processing::registered(&conn, first, None, "failed", Some("first failure")).unwrap();
    let second =
        db::processing::register(&conn, "retry", std::path::Path::new("/retry.png"), None).unwrap();
    db::processing::registered(&conn, second, None, "completed", None).unwrap();
    db::processing::registered(&conn, first, None, "running", None).unwrap();
    let before = db::processing::summary(&conn).unwrap();
    assert_eq!(before.active_count, 0);
    assert_eq!(before.counts.get("success"), Some(&1));
    {
        let tx = conn.transaction().unwrap();
        db::processing::registered(&tx, second, None, "failed", Some("rollback")).unwrap();
        let status: String = tx
            .query_row("SELECT status FROM processing_items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(status, "attention");
        tx.rollback().unwrap();
    }
    let after = db::processing::summary(&conn).unwrap();
    assert_eq!(after.counts, before.counts);
    assert_eq!(after.revision, before.revision);
    let now = clip_studio_lib::util::now();
    conn.execute(
        "UPDATE processing_history SET updated_at=?",
        [now - db::processing::RETENTION_SECONDS - 1],
    )
    .unwrap();
    assert_eq!(db::processing::cleanup(&conn, now, 1).unwrap(), 1);
    assert_eq!(
        db::processing::summary(&conn).unwrap().counts,
        before.counts,
        "purging history cannot alter current state"
    );
    let history = db::processing::history(&conn, "item:import:retry:/retry.png", None).unwrap();
    assert_eq!(history.tasks.len(), 1);
    assert_eq!(history.tasks[0].id, second);
    assert_eq!(history.tasks[0].status, "completed");
    conn.execute(
        "UPDATE processing_tasks SET updated_at=? WHERE id=?",
        rusqlite::params![now - db::processing::RETENTION_SECONDS - 1, second],
    )
    .unwrap();
    assert_eq!(db::processing::cleanup(&conn, now, 1).unwrap(), 1);
    assert!(db::processing::summary(&conn).unwrap().counts.is_empty());
}

#[test]
fn history_is_cursor_paginated_while_list_only_returns_current_steps() {
    let (_dir, s) = setup();
    let mut conn = s.db.lock().unwrap();
    let tx = conn.transaction().unwrap();
    for attempt in 1..=205 {
        db::processing::record_attempt(
            &tx,
            db::processing::NewAttempt {
                item_key: "history".into(),
                batch_id: "batch".into(),
                file_name: "history.png".into(),
                source_path: "/history.png".into(),
                kind: "import".into(),
                status: "completed".into(),
                phase: "done".into(),
                attempt,
                created_at: 1,
                updated_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
    }
    tx.commit().unwrap();
    let result = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.items[0].tasks.len(), 1);
    assert_eq!(result.items[0].tasks[0].attempt, 205);
    let first = db::processing::history(&conn, &result.items[0].key, None).unwrap();
    let second = db::processing::history(&conn, &result.items[0].key, first.next_cursor).unwrap();
    let third = db::processing::history(&conn, &result.items[0].key, second.next_cursor).unwrap();
    assert_eq!(
        (first.tasks.len(), second.tasks.len(), third.tasks.len()),
        (100, 100, 5)
    );
    assert!(third.next_cursor.is_none());
    assert_eq!(third.tasks.last().unwrap().attempt, 1);
}

#[test]
fn large_history_does_not_change_the_bounded_list_payload() {
    let (_dir, s) = setup();
    let mut conn = s.db.lock().unwrap();
    for attempt in 1..=100 {
        let tx = conn.transaction().unwrap();
        {
            for item in 0..1000 {
                let name = format!("material-{item}");
                db::processing::record_attempt(
                    &tx,
                    db::processing::NewAttempt {
                        item_key: name.clone(),
                        batch_id: "large".into(),
                        file_name: name.clone(),
                        source_path: name,
                        kind: "import".into(),
                        status: if item % 2 == 0 { "failed" } else { "completed" }.into(),
                        phase: "done".into(),
                        attempt,
                        created_at: 1,
                        updated_at: attempt,
                        ..Default::default()
                    },
                )
                .unwrap();
            }
        }
        tx.commit().unwrap();
        if [10, 50, 100].contains(&attempt) {
            let mut times = Vec::new();
            for _ in 0..3 {
                let started = std::time::Instant::now();
                let result = db::processing::list(
                    &conn,
                    &db::processing::Filter {
                        status: "attention".into(),
                        ..Default::default()
                    },
                )
                .unwrap();
                times.push(started.elapsed().as_secs_f64() * 1000.0);
                assert_eq!(result.total, 500);
                assert_eq!(result.items.len(), 20);
                assert!(result
                    .items
                    .iter()
                    .all(|i| i.tasks.len() == 1 && i.tasks[0].attempt == attempt));
            }
            times.sort_by(f64::total_cmp);
            println!(
                "processing list: {} attempts / 1000 materials: median {:.2} ms",
                attempt * 1000,
                times[1]
            );
        }
    }
}

#[test]
fn queue_schema_survives_the_online_sources_migration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.db");
    let conn = db::open_library_db(&path).unwrap();
    assert_eq!(db::SCHEMA_VERSION, 24);
    let task =
        db::processing::register(&conn, "batch", std::path::Path::new("/new.png"), None).unwrap();
    db::processing::registered(&conn, task, None, "failed", Some("source missing")).unwrap();
    let revision = db::processing::summary(&conn).unwrap().revision;
    drop(conn);
    let conn = db::open_library_db(&path).unwrap();
    assert_eq!(
        db::processing::summary(&conn).unwrap().revision,
        revision,
        "reopening must not rebuild summaries"
    );
    assert_eq!(
        db::processing::list(&conn, &Default::default())
            .unwrap()
            .total,
        1
    );
    assert!(
        !dir.path().join("backups").exists(),
        "fresh schema needs no intermediate upgrade"
    );
}

#[test]
fn importing_a_broken_file_preserves_the_individual_failure_after_reopen() {
    let (_dir, s) = setup();
    let sources = tempfile::tempdir().unwrap();
    let file = sources.path().join("broken.png");
    std::fs::write(&file, b"not a png").unwrap();
    let (progress, on_asset) = import::noop_cbs();
    let summary = import::run_import_with_policy(
        &s,
        &[file],
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        db::indexing::IndexPolicy {
            visual: false,
            subtitles: false,
        },
        clip_studio_lib::import::ImportCallbacks {
            progress: &progress,
            on_asset: &on_asset,
        },
    );
    assert_eq!(summary.failed, 1);
    let conn = s.read_db();
    let recorded:i64=conn.query_row("SELECT COUNT(*) FROM processing_tasks WHERE kind='import' AND status='failed' AND error IS NOT NULL",[],|r|r.get(0)).unwrap();
    assert_eq!(recorded, 1, "each broken material must be retained");
}

fn page(s: &library::LibrarySession, status: &str) -> db::processing::Page {
    db::processing::list(
        &s.read_db(),
        &db::processing::Filter {
            status: status.into(),
            ..Default::default()
        },
    )
    .unwrap()
}
fn insert(s: &library::LibrarySession, name: &str) -> i64 {
    db::assets::insert_asset(
        &s.db.lock().unwrap(),
        clip_studio_lib::db::assets::NewAsset {
            folder_id: None,
            file_name: name,
            rel_path: name,
            file_hash: name,
            file_size: 1,
            mime_type: "image/png",
            kind: "image",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap()
}

#[test]
fn queue_groups_legacy_and_new_operations_by_material_without_losing_history() {
    let (_dir, s) = setup();
    let a = insert(&s, "same.png");
    let b = insert(&s, "different-content.png");
    let conn = s.db.lock().unwrap();
    conn.execute("UPDATE assets SET file_name='same.png' WHERE id=?", [b])
        .unwrap();
    for (key, asset, kind, status) in [
        ("legacy-index-1", a, "visual", "completed"),
        ("legacy-index-2", a, "subtitles", "failed"),
        ("build:new:1", a, "subtitles", "completed"),
        ("legacy-index-3", b, "visual", "failed"),
    ] {
        db::processing::record_attempt(
            &conn,
            db::processing::NewAttempt {
                item_key: key.into(),
                batch_id: "legacy-index".into(),
                asset_id: Some(asset),
                file_name: "same.png".into(),
                source_path: key.into(),
                kind: kind.into(),
                status: status.into(),
                phase: kind.into(),
                attempt: 1,
                created_at: 1,
                updated_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
    }
    let result = db::processing::list(&conn, &db::processing::Filter::default()).unwrap();
    assert_eq!(result.total, 2, "one row per asset, never by file name");
    let row = result.items.iter().find(|i| i.asset_id == Some(a)).unwrap();
    assert_eq!(row.status, "success", "old failure belongs in history");
    assert_eq!(row.tasks.len(), 2);
    assert_eq!(
        db::processing::history(&conn, &row.key, None)
            .unwrap()
            .tasks
            .len(),
        3
    );
    assert_eq!(result.counts.get("attention"), Some(&1));
    assert_eq!(
        db::processing::list(
            &conn,
            &db::processing::Filter {
                kind: "subtitles".into(),
                ..Default::default()
            }
        )
        .unwrap()
        .total,
        1
    );
    db::processing::hide(&conn, &row.key).unwrap();
    assert_eq!(
        db::processing::list(&conn, &db::processing::Filter::default())
            .unwrap()
            .total,
        1
    );
    let history: i64 = conn
        .query_row(
            "SELECT (SELECT COUNT(*) FROM processing_tasks WHERE asset_id=?1)+(SELECT COUNT(*) FROM processing_history WHERE asset_id=?1)",
            [a],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(history, 3, "clearing must not delete history");
}

#[test]
fn source_only_failure_joins_the_actual_asset_after_retry_and_keeps_its_name() {
    let (_dir, s) = setup();
    let asset = insert(&s, "original.png");
    let conn = s.db.lock().unwrap();
    let first =
        db::processing::register(&conn, "batch", std::path::Path::new("/renamed.png"), None)
            .unwrap();
    db::processing::registered(&conn, first, None, "failed", Some("missing")).unwrap();
    let retry =
        db::processing::register(&conn, "batch", std::path::Path::new("/renamed.png"), None)
            .unwrap();
    db::processing::registered(&conn, retry, Some(asset), "skipped", None).unwrap();
    let result = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].file_name, "original.png");
    assert_eq!(result.items[0].tasks.len(), 1);
    assert_eq!(result.items[0].status, "success");
    let history = db::processing::history(&conn, &result.items[0].key, None).unwrap();
    assert_eq!(history.tasks.len(), 2);
    assert!(history
        .tasks
        .iter()
        .any(|t| t.error.as_deref() == Some("missing")));
}

#[test]
fn mixed_batch_keeps_success_failure_and_duplicate_as_separate_materials() {
    let (_dir, s) = setup();
    let source = tempfile::tempdir().unwrap();
    let good = source.path().join("good.png");
    let bad = source.path().join("bad.png");
    image::RgbImage::from_pixel(12, 8, image::Rgb([255, 0, 0]))
        .save(&good)
        .unwrap();
    std::fs::write(&bad, b"broken").unwrap();
    let (progress, on_asset) = import::noop_cbs();
    import::run_import_with_policy(
        &s,
        &[good.clone(), bad],
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        db::indexing::IndexPolicy {
            visual: false,
            subtitles: false,
        },
        clip_studio_lib::import::ImportCallbacks {
            progress: &progress,
            on_asset: &on_asset,
        },
    );
    let result = page(&s, "all");
    assert_eq!(result.total, 2);
    assert_eq!(result.counts.get("success"), Some(&1));
    assert_eq!(result.counts.get("attention"), Some(&1));
    let summary = import::run_import_with_policy(
        &s,
        &[good],
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        db::indexing::IndexPolicy {
            visual: false,
            subtitles: false,
        },
        clip_studio_lib::import::ImportCallbacks {
            progress: &progress,
            on_asset: &on_asset,
        },
    );
    assert_eq!(summary.duplicates, 1);
    assert_eq!(
        page(&s, "all")
            .items
            .iter()
            .flat_map(|i| &i.tasks)
            .filter(|t| t.status == "skipped")
            .count(),
        1
    );
}

#[test]
fn missing_source_is_recorded_even_without_an_asset_id() {
    let (_dir, s) = setup();
    let missing = s.root.join("never-existed.png");
    let (progress, on_asset) = import::noop_cbs();
    let result = import::run_import_with_policy(
        &s,
        std::slice::from_ref(&missing),
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        db::indexing::IndexPolicy {
            visual: false,
            subtitles: false,
        },
        clip_studio_lib::import::ImportCallbacks {
            progress: &progress,
            on_asset: &on_asset,
        },
    );
    assert_eq!(result.failed, 1);
    let result = page(&s, "attention");
    assert_eq!(result.total, 1);
    assert_eq!(
        result.total, 1,
        "changing the group key must remove the old material summary"
    );
    assert!(result.items[0].asset_id.is_none());
    assert_eq!(result.items[0].source_path, missing.to_string_lossy());
    assert!(result.items[0].tasks[0].error.is_some());
}

#[test]
fn manual_index_build_records_each_asset_and_deduplicates_active_work() {
    let (_dir, s) = setup();
    let a = insert(&s, "one.png");
    let b = insert(&s, "two.png");
    let scope = db::indexing::IndexScope {
        all: true,
        ..Default::default()
    };
    let conn = s.db.lock().unwrap();
    assert_eq!(
        db::indexing::enqueue_scope(&conn, &scope, &["visual".into()]).unwrap(),
        2
    );
    assert_eq!(
        db::indexing::enqueue_scope(&conn, &scope, &["visual".into()]).unwrap(),
        0
    );
    drop(conn);
    let result = page(&s, "processing");
    assert_eq!(result.total, 2);
    assert!(result.items.iter().all(|i| i
        .tasks
        .iter()
        .any(|t| t.kind == "visual" && t.status == "queued")));
    for id in [a, b] {
        let asset = db::assets::get_asset(&s.read_db(), &s.root, id)
            .unwrap()
            .unwrap();
        assert_eq!(asset.processing_tasks[0].status, "queued");
    }
}

#[test]
fn automatic_indexes_attach_to_import_and_do_not_mark_the_material_complete_early() {
    let (_dir, s) = setup();
    let id = insert(&s, "a.png");
    let conn = s.db.lock().unwrap();
    let record =
        db::processing::register(&conn, "batch", std::path::Path::new("/source/a.png"), None)
            .unwrap();
    db::processing::registered(&conn, record, Some(id), "running", None).unwrap();
    db::indexing::enqueue(&conn, id, "visual").unwrap();
    db::assets::set_import_stage(&conn, id, "done").unwrap();
    drop(conn);
    let result = page(&s, "processing");
    assert_eq!(result.total, 1);
    assert_eq!(result.items[0].tasks.len(), 2);
    assert_eq!(
        result.items[0]
            .tasks
            .iter()
            .find(|t| t.kind == "import")
            .unwrap()
            .status,
        "completed"
    );
}

#[test]
fn retry_retains_previous_index_error_but_card_only_exposes_the_latest_attempt() {
    let (_dir, s) = setup();
    let id = insert(&s, "a.png");
    let conn = s.db.lock().unwrap();
    db::indexing::enqueue(&conn, id, "visual").unwrap();
    index_status(&conn, "failed", Some("model unavailable"));
    assert_eq!(
        db::processing::list(&conn, &db::processing::Filter::default())
            .unwrap()
            .counts
            .get("attention"),
        Some(&1)
    );
    index_status(&conn, "queued", None);
    index_status(&conn, "running", None);
    index_status(&conn, "completed", None);
    let result = db::processing::list(&conn, &db::processing::Filter::default()).unwrap();
    assert_eq!(result.items[0].status, "success");
    assert_eq!(result.items[0].tasks.len(), 1);
    let history = db::processing::history(&conn, &result.items[0].key, None).unwrap();
    assert_eq!(history.tasks.len(), 2);
    assert_eq!(history.tasks[1].error.as_deref(), Some("model unavailable"));
    assert_eq!(result.items[0].tasks[0].attempt, 2);
    let asset = db::assets::get_asset(&conn, &s.root, id).unwrap().unwrap();
    assert_eq!(asset.processing_tasks.len(), 1);
    assert_eq!(asset.processing_tasks[0].status, "completed");
}

#[test]
fn import_retry_preserves_history_and_tracks_processing_phases() {
    let (_dir, s) = setup();
    let id = insert(&s, "a.png");
    let conn = s.db.lock().unwrap();
    let record =
        db::processing::register(&conn, "batch", std::path::Path::new("/a.png"), None).unwrap();
    db::processing::registered(&conn, record, Some(id), "running", None).unwrap();
    db::assets::set_import_stage(&conn, id, "thumb").unwrap();
    db::assets::set_import_stage(&conn, id, "error:bad image").unwrap();
    db::processing::retry_import(&conn, id).unwrap();
    assert!(db::processing::retry_import(&conn, id).is_err());
    db::assets::set_import_stage(&conn, id, "probing").unwrap();
    db::assets::set_import_stage(&conn, id, "done").unwrap();
    let result = db::processing::list(&conn, &db::processing::Filter::default()).unwrap();
    assert_eq!(result.items[0].status, "success");
    let history = db::processing::history(&conn, &result.items[0].key, None).unwrap();
    assert_eq!(history.tasks[1].phase, "thumb");
    assert_eq!(history.tasks[1].error.as_deref(), Some("bad image"));
    assert_eq!(result.items[0].tasks[0].phase, "done");
}

#[test]
fn restart_retains_history_marks_import_interrupted_and_resumes_index_work() {
    let (dir, s) = setup();
    let id = insert(&s, "a.png");
    let meta = s.meta.clone();
    {
        let conn = s.db.lock().unwrap();
        let record =
            db::processing::register(&conn, "batch", std::path::Path::new("/a.png"), None).unwrap();
        db::processing::registered(&conn, record, Some(id), "running", None).unwrap();
        db::indexing::enqueue(&conn, id, "visual").unwrap();
        index_status(&conn, "running", None);
    }
    drop(s);
    let reopened = library::open_session(meta).unwrap();
    let result = page(&reopened, "all");
    assert_eq!(
        result.items[0]
            .tasks
            .iter()
            .find(|t| t.kind == "import")
            .unwrap()
            .status,
        "interrupted"
    );
    assert_eq!(
        result.items[0]
            .tasks
            .iter()
            .find(|t| t.kind == "visual")
            .unwrap()
            .status,
        "queued"
    );
    assert!(dir.path().exists());
}

#[test]
fn clear_record_cannot_cancel_active_work_and_never_changes_asset_state() {
    let (_dir, s) = setup();
    let id = insert(&s, "a.png");
    let conn = s.db.lock().unwrap();
    db::indexing::enqueue(&conn, id, "visual").unwrap();
    let key = db::processing::list(&conn, &Default::default())
        .unwrap()
        .items[0]
        .key
        .clone();
    assert!(db::processing::hide(&conn, &key).is_err());
    index_status(&conn, "completed", None);
    db::processing::hide(&conn, &key).unwrap();
    assert_eq!(
        db::processing::list(&conn, &Default::default())
            .unwrap()
            .total,
        0
    );
    assert!(db::assets::get_asset(&conn, &s.root, id).unwrap().is_some());
}

#[test]
fn trash_cancels_work_but_permanent_delete_preserves_file_failure_history() {
    let (_dir, s) = setup();
    let id = insert(&s, "a.png");
    let conn = s.db.lock().unwrap();
    db::indexing::enqueue(&conn, id, "visual").unwrap();
    conn.execute("UPDATE assets SET deleted_at=123 WHERE id=?", [id])
        .unwrap();
    assert_eq!(
        db::processing::list(&conn, &Default::default())
            .unwrap()
            .items[0]
            .status,
        "cancelled"
    );
    conn.execute("DELETE FROM assets WHERE id=?", [id]).unwrap();
    let result = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(
        result.total, 1,
        "changing the group key must remove the old material summary"
    );
    assert!(result.items[0].asset_id.is_none());
    assert_eq!(result.items[0].file_name, "a.png");
}

#[test]
fn pagination_and_type_filters_do_not_lose_failures_beyond_two_hundred_records() {
    let (_dir, s) = setup();
    let mut conn = s.db.lock().unwrap();
    let tx = conn.transaction().unwrap();
    for n in 0..10001 {
        let id = db::processing::register(
            &tx,
            "large",
            std::path::Path::new(&format!("/source/{n}.png")),
            None,
        )
        .unwrap();
        db::processing::registered(&tx, id, None, "failed", Some("copy denied")).unwrap();
    }
    tx.commit().unwrap();
    let result = db::processing::list(
        &conn,
        &db::processing::Filter {
            status: "attention".into(),
            kind: "import".into(),
            page: 101,
            page_size: 100,
        },
    )
    .unwrap();
    assert_eq!(result.total, 10001);
    assert_eq!(result.items.len(), 1);
    assert_eq!(result.counts.get("attention"), Some(&10001));
    assert!(db::processing::list(
        &conn,
        &db::processing::Filter {
            kind: "bad".into(),
            ..Default::default()
        }
    )
    .is_err());
}

fn index_status(conn: &rusqlite::Connection, status: &str, error: Option<&str>) {
    db::indexing::set_status(
        conn,
        None,
        status,
        error,
        &[
            "queued",
            "running",
            "failed",
            "completed",
            "cancelled",
            "no_source",
        ],
    )
    .unwrap();
}

#[test]
fn current_expiry_does_not_depend_on_recent_old_attempts() {
    let (_dir, s) = setup();
    let conn = s.db.lock().unwrap();
    let first =
        db::processing::register(&conn, "independent", std::path::Path::new("/old.png"), None)
            .unwrap();
    db::processing::registered(&conn, first, None, "failed", Some("old error")).unwrap();
    let second =
        db::processing::register(&conn, "independent", std::path::Path::new("/old.png"), None)
            .unwrap();
    db::processing::registered(&conn, second, None, "completed", None).unwrap();
    // A late old worker is audit-only, even if its timestamp is newer than current work.
    db::processing::registered(&conn, first, None, "running", None).unwrap();
    let now = clip_studio_lib::util::now();
    conn.execute(
        "UPDATE processing_tasks SET updated_at=? WHERE id=?",
        rusqlite::params![now - db::processing::RETENTION_SECONDS - 1, second],
    )
    .unwrap();
    assert_eq!(db::processing::cleanup(&conn, now, 1).unwrap(), 1);
    assert!(db::processing::summary(&conn).unwrap().counts.is_empty());
    assert!(db::processing::get(&conn, second).unwrap().is_none());
    let history = db::processing::history(&conn, "item:import:independent:/old.png", None).unwrap();
    assert_eq!(history.tasks.len(), 1);
    assert_eq!(history.tasks[0].id, first);
    assert_eq!(history.tasks[0].status, "running");
    assert_eq!(
        db::processing::cleanup(&conn, now + db::processing::RETENTION_SECONDS + 1, 1).unwrap(),
        1
    );
    assert!(db::processing::summary(&conn).unwrap().counts.is_empty());
}

#[test]
fn failed_attempt_creation_rolls_back_archiving_and_revision() {
    let (_dir, s) = setup();
    let conn = s.db.lock().unwrap();
    let first =
        db::processing::register(&conn, "atomic", std::path::Path::new("/atomic.png"), None)
            .unwrap();
    db::processing::registered(&conn, first, None, "failed", Some("original")).unwrap();
    let before = db::processing::summary(&conn).unwrap();
    conn.execute_batch("CREATE TEMP TRIGGER reject_attempt BEFORE INSERT ON processing_tasks WHEN NEW.source_path='reject' BEGIN SELECT RAISE(ABORT,'injected insert failure'); END;").unwrap();
    assert!(db::processing::record_attempt(
        &conn,
        db::processing::NewAttempt {
            item_key: "import:atomic:/atomic.png".into(),
            batch_id: "atomic".into(),
            file_name: "atomic.png".into(),
            source_path: "reject".into(),
            kind: "import".into(),
            status: "queued".into(),
            phase: "copy".into(),
            attempt: 2,
            created_at: 1,
            updated_at: 1,
            ..Default::default()
        }
    )
    .is_err());
    let after = db::processing::summary(&conn).unwrap();
    assert_eq!(after.revision, before.revision);
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(page.items[0].tasks[0].id, first);
    assert_eq!(page.items[0].tasks[0].error.as_deref(), Some("original"));
    assert_eq!(
        db::processing::history(&conn, &page.items[0].key, None)
            .unwrap()
            .tasks
            .len(),
        1
    );
}

#[test]
fn execution_transition_rolls_back_if_current_update_fails() {
    let (_dir, s) = setup();
    let asset = insert(&s, "transaction.png");
    let conn = s.db.lock().unwrap();
    db::indexing::enqueue(&conn, asset, "visual").unwrap();
    let before = db::processing::summary(&conn).unwrap();
    conn.execute_batch("CREATE TEMP TRIGGER reject_transition BEFORE UPDATE ON processing_tasks WHEN NEW.status='running' BEGIN SELECT RAISE(ABORT,'injected snapshot failure'); END;").unwrap();
    assert!(db::indexing::set_status(&conn, None, "running", None, &["queued"]).is_err());
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "queued");
    assert_eq!(
        db::processing::list(&conn, &Default::default())
            .unwrap()
            .items[0]
            .tasks[0]
            .status,
        "queued"
    );
    assert_eq!(
        db::processing::summary(&conn).unwrap().revision,
        before.revision
    );
}

#[test]
fn clearing_then_retrying_does_not_reveal_cleared_history() {
    let (_dir, s) = setup();
    let asset = insert(&s, "hidden.png");
    let conn = s.db.lock().unwrap();
    let first =
        db::processing::register(&conn, "hidden", std::path::Path::new("/hidden.png"), None)
            .unwrap();
    db::processing::registered(&conn, first, Some(asset), "failed", Some("old failure")).unwrap();
    db::processing::retry_import(&conn, asset).unwrap();
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    let second = page.items[0].tasks[0].id;
    db::processing::registered(&conn, second, Some(asset), "completed", None).unwrap();
    assert_eq!(db::processing::clear_finished(&conn).unwrap(), 1);
    db::processing::retry_import(&conn, asset).unwrap();
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(page.total, 1);
    let history = db::processing::history(&conn, &page.items[0].key, None).unwrap();
    assert_eq!(history.tasks.len(), 1);
    assert_eq!(history.tasks[0].status, "queued");
    assert_eq!(history.tasks[0].attempt, 3);
}

#[test]
fn late_index_worker_cannot_fail_a_new_retry_of_the_same_execution_task() {
    let (_dir, s) = setup();
    let asset = insert(&s, "generation.png");
    let conn = s.db.lock().unwrap();
    db::indexing::enqueue(&conn, asset, "visual").unwrap();
    let old = db::processing::list(&conn, &Default::default())
        .unwrap()
        .items[0]
        .tasks[0]
        .clone();
    let index = old.index_task_id.unwrap();
    db::indexing::set_status(&conn, Some(index), "running", None, &["queued"]).unwrap();
    db::indexing::set_status(&conn, Some(index), "cancelled", None, &["running"]).unwrap();
    db::indexing::set_status(&conn, Some(index), "queued", None, &["cancelled"]).unwrap();
    db::indexing::set_status(&conn, Some(index), "running", None, &["queued"]).unwrap();
    assert_eq!(
        db::indexing::set_status_if_current(
            &conn,
            index,
            "failed",
            Some("late old failure"),
            &["running"],
            old.id
        )
        .unwrap(),
        0
    );
    let page = db::processing::list(&conn, &Default::default()).unwrap();
    assert_eq!(page.items[0].tasks[0].status, "running");
    assert_eq!(page.items[0].tasks[0].attempt, 2);
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "running");
}
