use super::*;
fn setup() -> (tempfile::TempDir, LibrarySession, i64, i64, i64) {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Publish").unwrap();
    let session = crate::library::open_session_light(meta).unwrap();
    let (asset, task, attempt) = {
        let conn = session.db.lock().unwrap();
        let id = db::assets::insert_asset(
            &conn,
            crate::db::assets::NewAsset {
                folder_id: None,
                file_name: "movie.mp4",
                rel_path: "movie.mp4",
                file_hash: "movie",
                file_size: 1,
                mime_type: "video/mp4",
                kind: "video",
                imported_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
        db::indexing::enqueue(&conn, id, "subtitles").unwrap();
        crate::db::indexing::set_status(&conn, None, "running", None, &["queued", "running"])
            .unwrap();
        (
            id,
            conn.query_row("SELECT id FROM index_tasks WHERE asset_id=?", [id], |r| {
                r.get(0)
            })
            .unwrap(),
            conn.query_row(
                "SELECT id FROM processing_tasks WHERE asset_id=?",
                [id],
                |r| r.get(0),
            )
            .unwrap(),
        )
    };
    (temp, session, asset, task, attempt)
}
#[test]
fn a_superseded_task_cannot_clear_the_replacement_subtitle_index() {
    let (_temp, session, id, task, attempt) = setup();
    {
        let conn = session.db.lock().unwrap();
        crate::db::indexing::set_status(
            &conn,
            Some(task),
            "cancelled",
            None,
            &["queued", "running"],
        )
        .unwrap();
        conn.execute("UPDATE assets SET subtitle_indexed=1 WHERE id=?", [id])
            .unwrap();
    }
    publish_result(
        &session,
        task,
        id,
        "subtitles",
        (1, std::time::UNIX_EPOCH),
        false,
        attempt,
    )
    .unwrap();
    assert!(
        db::assets::get_asset(&session.read_db(), &session.root, id)
            .unwrap()
            .unwrap()
            .subtitle_indexed
    );
    assert_eq!(
        db::indexing::tasks(&session.read_db()).unwrap()[0].status,
        "cancelled"
    );
}
#[test]
fn cancellation_does_not_publish_an_unfinished_result() {
    let (_temp, session, id, task, attempt) = setup();
    crate::db::indexing::set_status(
        &session.db.lock().unwrap(),
        Some(task),
        "cancelled",
        None,
        &["queued", "running"],
    )
    .unwrap();
    publish_result(
        &session,
        task,
        id,
        "subtitles",
        (1, std::time::UNIX_EPOCH),
        false,
        attempt,
    )
    .unwrap();
    assert!(
        !db::assets::get_asset(&session.read_db(), &session.root, id)
            .unwrap()
            .unwrap()
            .subtitle_indexed
    );
}
#[test]
fn source_changes_fail_the_task_without_marking_a_result_ready() {
    let (_temp, session, id, task, attempt) = setup();
    publish_result(
        &session,
        task,
        id,
        "subtitles",
        (1, std::time::UNIX_EPOCH),
        true,
        attempt,
    )
    .unwrap();
    assert!(
        !db::assets::get_asset(&session.read_db(), &session.root, id)
            .unwrap()
            .unwrap()
            .subtitle_indexed
    );
    assert_eq!(
        db::indexing::tasks(&session.read_db()).unwrap()[0].status,
        "failed"
    );
}

#[test]
fn successful_publish_records_the_fingerprint_and_does_not_enable_the_other_index() {
    let (_temp, session, id, task, attempt) = setup();
    // Windows SystemTime represents FILETIME ticks with 100 ns precision.
    let modified = std::time::UNIX_EPOCH + Duration::from_nanos(123456700);
    publish_result(
        &session,
        task,
        id,
        "subtitles",
        (42, modified),
        false,
        attempt,
    )
    .unwrap();
    let conn = session.read_db();
    let asset = db::assets::get_asset(&conn, &session.root, id)
        .unwrap()
        .unwrap();
    assert!(asset.subtitle_indexed);
    assert!(!asset.visual_indexed);
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "completed");
    let version: (String, i64, String) = conn.query_row(
        "SELECT recipe,source_size,source_mtime FROM index_versions WHERE asset_id=? AND kind='subtitles'",
        [id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    ).unwrap();
    assert_eq!(version, ("1".into(), 42, "123456700".into()));
    // A delayed duplicate callback cannot overwrite a committed generation.
    publish_result(
        &session,
        task,
        id,
        "subtitles",
        (99, modified),
        true,
        attempt,
    )
    .unwrap();
    assert!(
        db::assets::get_asset(&conn, &session.root, id)
            .unwrap()
            .unwrap()
            .subtitle_indexed
    );
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "completed");
}

#[test]
fn trashing_an_asset_before_publish_cannot_record_a_completed_generation() {
    let (_temp, session, id, task, attempt) = setup();
    session
        .db
        .lock()
        .unwrap()
        .execute("UPDATE assets SET deleted_at=1 WHERE id=?", [id])
        .unwrap();
    publish_result(
        &session,
        task,
        id,
        "subtitles",
        (1, std::time::UNIX_EPOCH),
        false,
        attempt,
    )
    .unwrap();
    let conn = session.read_db();
    assert!(
        !db::assets::get_asset(&conn, &session.root, id)
            .unwrap()
            .unwrap()
            .subtitle_indexed
    );
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "cancelled");
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM index_versions WHERE asset_id=?",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}

#[test]
fn publication_database_failure_rolls_back_ready_flag_generation_and_task_completion() {
    let (_temp, session, id, task, attempt) = setup();
    session.db.lock().unwrap().execute_batch("CREATE TRIGGER p0_publish_failure BEFORE UPDATE OF status ON index_tasks WHEN NEW.status='completed' BEGIN SELECT RAISE(ABORT,'P0 publish failure'); END;").unwrap();
    let error = publish_result(
        &session,
        task,
        id,
        "subtitles",
        (
            321,
            std::time::UNIX_EPOCH + std::time::Duration::from_secs(17),
        ),
        false,
        attempt,
    )
    .unwrap_err();
    assert!(error.to_string().contains("P0 publish failure"));
    let conn = session.read_db();
    assert!(
        !db::assets::get_asset(&conn, &session.root, id)
            .unwrap()
            .unwrap()
            .subtitle_indexed
    );
    assert_eq!(db::indexing::tasks(&conn).unwrap()[0].status, "running");
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM index_versions WHERE asset_id=?",
            [id],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
}
