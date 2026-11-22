use super::*;

#[test]
fn recovery_discards_partial_copies_and_duplicate_links_only_with_an_intact_original() {
    for published in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let meta = crate::library::init_library(temp.path(), "File journal").unwrap();
        let session = crate::library::open_session(meta).unwrap();
        let source = temp.path().join("images/source.jpg");
        let target = temp.path().join(".clipstudio-open/trash/target.jpg");
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(&source, b"complete original").unwrap();
        let journal = Journal {
            committed: false,
            ids: vec![],
            moves: vec![Journal::plan(temp.path(), &source, &target).unwrap()],
        };
        journal
            .save(&session.db.lock().unwrap(), "asset_lifecycle.v1.partial")
            .unwrap();
        let stage = stage_path(&source, &target);
        fs::write(&stage, b"incomplete").unwrap();
        if published {
            fs::hard_link(&source, &target).unwrap();
        }
        recover(&session).unwrap();
        recover(&session).unwrap();
        assert_eq!(fs::read(&source).unwrap(), b"complete original");
        assert!(!target.exists());
        assert!(!stage.exists());
    }
}

#[test]
fn recovery_rejects_outside_paths_without_touching_files_or_discarding_the_journal() {
    let temp = tempfile::tempdir().unwrap();
    let external = tempfile::tempdir().unwrap();
    let outside = external.path().join("source");
    fs::write(&outside, b"external original").unwrap();
    let meta = crate::library::init_library(temp.path(), "File journal").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    let journal = Journal {
        committed: false,
        ids: vec![],
        moves: vec![Move {
            source: outside.to_string_lossy().into_owned(),
            target: "images/target.jpg".into(),
            hash: hash(&outside).unwrap(),
        }],
    };
    journal
        .save(&session.db.lock().unwrap(), "asset_lifecycle.v1.invalid")
        .unwrap();
    assert!(recover(&session).is_err());
    assert_eq!(fs::read(outside).unwrap(), b"external original");
    assert!(!temp.path().join("images/target.jpg").exists());
    assert_eq!(
        session
            .read_db()
            .query_row(
                "SELECT COUNT(*) FROM meta WHERE key='asset_lifecycle.v1.invalid'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
}
