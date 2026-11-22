use super::*;

#[test]
fn registration_checks_activity_before_copying_or_inserting_a_row() {
    let mode = ImportMode::Reference;
    {
        let temp = tempfile::tempdir().unwrap();
        let meta =
            crate::library::init_library(&temp.path().join("library"), "Import activity").unwrap();
        let session = Arc::new(crate::library::open_session(meta).unwrap());
        let input = temp.path().join("input.svg");
        fs::write(
            &input,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8"/>"#,
        )
        .unwrap();
        let staging = LibraryDirs::new(&session.root).app().join("importing");
        fs::create_dir_all(&staging).unwrap();
        let cleanup = session.asset_activity.cleanup().unwrap();
        assert!(prepare_one(&session, &input, &staging, None, mode, &|_| {}).is_err());
        assert_eq!(
            session
                .read_db()
                .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(fs::read_dir(&staging).unwrap().count(), 0);
        drop(cleanup);
        assert!(prepare_one(&session, &input, &staging, None, mode, &|_| {}).is_ok());
    }
}
