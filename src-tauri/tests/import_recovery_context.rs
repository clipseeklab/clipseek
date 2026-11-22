//! Persisted import intent must survive process restart independently of UI state.
use clip_studio_lib::db::{self, import_jobs};

fn save_legacy_job_with_tag_intent(conn: &rusqlite::Connection) {
    import_jobs::start(
        conn,
        "tag-job",
        "paths",
        &["/source/photo.jpg".into()],
        Some(7),
    )
    .unwrap();
    conn.execute(
        "INSERT INTO meta(key,value) VALUES('import_job_tags.v1.tag-job',?1)",
        [r#"["旅行","收藏"]"#],
    )
    .unwrap();
}

#[test]
fn recovered_job_exposes_its_persisted_tags_after_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.db");
    {
        let conn = db::open_library_db(&path).unwrap();
        save_legacy_job_with_tag_intent(&conn);
    }
    let conn = db::open_library_db(&path).unwrap();
    let job = serde_json::to_value(import_jobs::get(&conn, "tag-job").unwrap().unwrap()).unwrap();
    assert_eq!(job["tags"], serde_json::json!(["旅行", "收藏"]));
    assert_eq!(job["folderId"], 7);
}

#[test]
fn finishing_or_abandoning_a_job_removes_its_tag_intent() {
    for status in ["completed", "failed", "abandoned"] {
        let temp = tempfile::tempdir().unwrap();
        let conn = db::open_library_db(&temp.path().join("library.db")).unwrap();
        save_legacy_job_with_tag_intent(&conn);
        if status == "abandoned" {
            import_jobs::abandon(&conn, "tag-job").unwrap();
        } else {
            import_jobs::finish(&conn, "tag-job", status, 1, 1, 0, 0).unwrap();
        }
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM meta WHERE key='import_job_tags.v1.tag-job'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
}

#[test]
fn legacy_jobs_have_no_tags_and_corrupt_context_is_reported_without_deleting_the_job() {
    let temp = tempfile::tempdir().unwrap();
    let conn = db::open_library_db(&temp.path().join("library.db")).unwrap();
    import_jobs::start(&conn, "old-job", "paths", &[], None).unwrap();
    let old = serde_json::to_value(import_jobs::get(&conn, "old-job").unwrap().unwrap()).unwrap();
    assert_eq!(old["tags"], serde_json::json!([]));
    conn.execute(
        "INSERT INTO meta(key,value) VALUES('import_job_tags.v1.old-job','invalid-json')",
        [],
    )
    .unwrap();
    assert!(import_jobs::get(&conn, "old-job").is_err());
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM import_jobs WHERE id='old-job'",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
}

#[test]
fn cleanup_failure_keeps_both_job_and_context_for_retry() {
    let temp = tempfile::tempdir().unwrap();
    let conn = db::open_library_db(&temp.path().join("library.db")).unwrap();
    save_legacy_job_with_tag_intent(&conn);
    conn.execute_batch("CREATE TRIGGER reject_context_cleanup BEFORE DELETE ON meta WHEN OLD.key LIKE 'import_job_tags.v1.%' BEGIN SELECT RAISE(ABORT, 'injected cleanup failure'); END;").unwrap();
    assert!(import_jobs::finish(&conn, "tag-job", "completed", 1, 1, 0, 0).is_err());
    assert!(import_jobs::get(&conn, "tag-job").unwrap().is_some());
    conn.execute_batch("DROP TRIGGER reject_context_cleanup")
        .unwrap();
    import_jobs::abandon(&conn, "tag-job").unwrap();
    assert!(import_jobs::get(&conn, "tag-job").unwrap().is_none());
}
