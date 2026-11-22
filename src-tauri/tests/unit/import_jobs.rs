use super::*;

#[test]
fn unfinished_job_survives_reopen_and_can_be_abandoned() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.db");
    {
        let conn = crate::db::open_library_db(&path).unwrap();
        start(
            &conn,
            "job-1",
            "paths",
            &["/source/a.jpg".into(), "/source/b.mov".into()],
            Some(7),
        )
        .unwrap();
    }

    let conn = crate::db::open_library_db(&path).unwrap();
    let jobs = recoverable(&conn).unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].paths.len(), 2);
    assert_eq!(jobs[0].folder_id, Some(7));
    abandon(&conn, "job-1").unwrap();
    assert!(recoverable(&conn).unwrap().is_empty());
}

#[test]
fn completed_job_is_not_offered_for_recovery() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    start(&conn, "job-2", "folders", &["/source/photos".into()], None).unwrap();
    finish(&conn, "job-2", "completed", 10, 8, 1, 1).unwrap();
    assert!(recoverable(&conn).unwrap().is_empty());
}

#[test]
fn restarting_a_job_updates_its_mode_and_paths() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    start(&conn, "job-3", "folders", &["/old/photo.png".into()], None).unwrap();
    start(&conn, "job-3", "paths", &["/new/photo.png".into()], Some(9)).unwrap();
    let job = get(&conn, "job-3").unwrap().unwrap();
    assert_eq!(job.mode, "paths");
    assert_eq!(job.paths, vec!["/new/photo.png"]);
    assert_eq!(job.folder_id, Some(9));
}

#[test]
fn submission_handoff_survives_job_completion_and_database_reopen() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("library.db");
    {
        let conn = crate::db::open_library_db(&path).unwrap();
        assert!(!has_submission(&conn, "capture-id").unwrap());
        start(
            &conn,
            "capture-id",
            "paths",
            &["/source/photo.png".into()],
            None,
        )
        .unwrap();
        assert!(has_submission(&conn, "capture-id").unwrap());
        let record = crate::db::processing::register(
            &conn,
            "capture-id",
            std::path::Path::new("/source/photo.png"),
            None,
        )
        .unwrap();
        crate::db::processing::registered(&conn, record, None, "failed", Some("source missing"))
            .unwrap();
        finish(&conn, "capture-id", "completed", 1, 0, 1, 0).unwrap();
        assert!(get(&conn, "capture-id").unwrap().is_none());
    }
    let conn = crate::db::open_library_db(&path).unwrap();
    assert!(
        has_submission(&conn, "capture-id").unwrap(),
        "processing history owns an accepted request after the transient job is gone"
    );
}
