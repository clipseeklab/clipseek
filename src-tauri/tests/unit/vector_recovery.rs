use super::*;

#[test]
fn empty_library_rebuild_completes_swap_without_loading_model() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "空库重建").unwrap();
    let session = Arc::new(crate::library::open_session(meta).unwrap());

    let report = rebuild_with_encoder(&session, "empty-test", &|_| {}, &|_| {
        panic!("An empty library must not invoke the encoder")
    })
    .unwrap();

    assert_eq!(report.indexed_frames, 0);
    assert_eq!(report.failed_frames, 0);
    assert!(session.vector.lock().unwrap().is_some());
    let app_dir = LibraryDirs::new(temp.path()).app();
    assert!(!app_dir.join(JOURNAL_FILE).exists());
    assert!(!app_dir.join("vector.backup-empty-test").exists());
    assert!(!app_dir.join("vector.rebuilding-empty-test").exists());
}

#[test]
fn free_space_estimate_does_not_count_the_existing_index_twice() {
    let current = 129 * 1024 * 1024;
    let required = estimated_rebuild_free_space(current, 10);

    assert_eq!(required, current * 2 + REBUILD_MIN_HEADROOM);
    assert!(required < current * 2 + 256 * 1024 * 1024);
}

#[test]
fn preflight_does_not_wait_for_the_main_database_mutex() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "预检并发测试").unwrap();
    let session = Arc::new(crate::library::open_session(meta).unwrap());
    let _writer_guard = session.db.lock().unwrap();
    let worker_session = session.clone();
    let (sender, receiver) = std::sync::mpsc::channel();

    std::thread::spawn(move || {
        sender.send(preflight(&worker_session)).unwrap();
    });

    let result = receiver
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("预检不应等待主写连接互斥锁")
        .unwrap();
    assert_eq!(result.frame_count, 0);
}

#[test]
fn deletion_during_rebuild_defers_file_cleanup_and_cannot_republish_readiness() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Rebuild cancellation").unwrap();
    let session = Arc::new(crate::library::open_session(meta).unwrap());
    let uid = crate::util::new_entity_uid();
    let directory = LibraryDirs::new(temp.path()).asset(1, &uid);
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("source.png");
    image::RgbImage::new(8, 8).save(&source).unwrap();
    let rel = crate::util::to_rel(temp.path(), &source);
    let id = {
        let conn = session.db.lock().unwrap();
        let id = crate::db::assets::insert_asset(
            &conn,
            crate::db::assets::NewAsset {
                folder_id: None,
                file_name: "source.png",
                rel_path: &rel,
                file_hash: &uid,
                file_size: 1,
                mime_type: "image/png",
                kind: "image",
                imported_at: 1,
                uid: Some(&uid),
                ..Default::default()
            },
        )
        .unwrap();
        crate::db::assets::insert_frame(&conn, id, 0, 0, Some(&rel)).unwrap();
        id
    };
    let deferred = std::cell::Cell::new(false);
    rebuild_with_encoder(&session, "deletion-race", &|_| {}, &|images| {
        deferred.set(crate::services::assets::delete_permanent(&session, &[id]).is_err());
        Ok(vec![vec![1.0; crate::clip::EMBED_DIM]; images.len()])
    })
    .unwrap();
    assert!(
        deferred.get(),
        "rebuild must keep ownership until its index swap completes"
    );
    assert!(source.is_file());
    let pending = crate::db::assets::get_asset(&session.read_db(), temp.path(), id)
        .unwrap()
        .unwrap();
    assert!(pending.deleted_at.is_some());
    assert!(
        !pending.visual_indexed,
        "a finished rebuild cannot republish a pending deletion"
    );
    crate::services::assets::delete_permanent(&session, &[id]).unwrap();
    assert!(!source.exists());
}
