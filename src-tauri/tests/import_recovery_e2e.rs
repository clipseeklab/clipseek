//! Import recovery tests rebuild interrupted media, avoid rebuilding embedding-ready inputs,
//! report missing sources, persist file/folder jobs across reopen, deduplicate resumed
//! registrations, and preserve registered assets when abandoning jobs.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clip_studio_lib::{db, import, library, vector};

fn setup_library() -> (tempfile::TempDir, Arc<library::LibrarySession>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let meta = library::init_library(&root, "恢复测试库").unwrap();
    let session = Arc::new(library::open_session(meta).unwrap());
    (temp, session)
}

fn make_image(path: &Path, color: [u8; 3]) {
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(96, 64, image::Rgb(color)))
        .save(path)
        .unwrap();
}

fn register(session: &Arc<library::LibrarySession>, paths: &[PathBuf]) -> import::RegisteredBatch {
    import::register_files(
        session,
        paths,
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    )
}

fn asset(session: &library::LibrarySession, id: i64) -> db::model::Asset {
    let conn = session.db.lock().unwrap();
    db::assets::get_asset(&conn, &session.root, id)
        .unwrap()
        .unwrap()
}

#[test]
fn photos_interrupted_at_multiple_stages_are_rebuilt_after_reopen() {
    let (temp, session) = setup_library();
    let sources = temp.path().join("sources");
    std::fs::create_dir_all(&sources).unwrap();
    let paths: Vec<_> = [[220, 40, 40], [40, 220, 40], [40, 40, 220]]
        .into_iter()
        .enumerate()
        .map(|(index, color)| {
            let path = sources.join(format!("photo-{index}.png"));
            make_image(&path, color);
            path
        })
        .collect();

    let registered = register(&session, &paths);
    let ids: Vec<_> = registered.pending.iter().map(|(id, _)| *id).collect();
    for (id, stage) in ids.iter().zip(["copied", "thumb", "frames"]) {
        let conn = session.db.lock().unwrap();
        db::assets::set_import_stage(&conn, *id, stage).unwrap();
    }

    // Simulate process exit by reopening the same library.
    let meta = session.meta.clone();
    drop(session);
    let reopened = Arc::new(library::open_session(meta).unwrap());
    let pending = {
        let conn = reopened.db.lock().unwrap();
        db::assets::recoverable_imports(&conn).unwrap()
    };
    assert_eq!(pending.len(), 3);

    let mut recovery = import::RegisteredBatch {
        duplicates: Vec::new(),
        pending,
        summary: import::ImportSummary {
            total: 3,
            ..Default::default()
        },
        aborted: false,
    };
    import::process_pending(
        &reopened,
        &mut recovery,
        &AtomicBool::new(false),
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(recovery.summary.done, 3);
    assert_eq!(recovery.summary.failed, 0);

    for id in ids {
        let restored = asset(&reopened, id);
        assert_eq!(restored.import_stage, "embedding");
        assert!(restored
            .thumb_path
            .as_ref()
            .is_some_and(|path| Path::new(path).is_file()));
        let conn = reopened.db.lock().unwrap();
        assert_eq!(db::assets::get_frame_ids(&conn, id).unwrap().len(), 1);
    }
}

#[test]
fn missing_library_file_becomes_error_instead_of_retrying_forever() {
    let (temp, session) = setup_library();
    let source = temp.path().join("missing.png");
    make_image(&source, [90, 120, 150]);
    let registered = register(&session, &[source]);
    let id = registered.pending[0].0;
    std::fs::remove_file(asset(&session, id).abs_path).unwrap();

    let (done, failed) = import::reprocess_assets(&session, &[id], &|_| {});
    assert_eq!((done, failed), (0, 1));
    assert!(asset(&session, id).import_stage.starts_with("error:"));
    let conn = session.db.lock().unwrap();
    assert!(db::assets::recoverable_imports(&conn).unwrap().is_empty());
}

#[test]
fn embedding_only_done_error_and_deleted_assets_are_not_media_rebuilt() {
    let (temp, session) = setup_library();
    let sources = temp.path().join("states");
    std::fs::create_dir_all(&sources).unwrap();
    let stages = ["embedding", "done", "error:broken", "copied"];
    let mut ids = Vec::new();
    for (index, stage) in stages.iter().enumerate() {
        let source = sources.join(format!("state-{index}.png"));
        make_image(&source, [20 + index as u8 * 30, 80, 160]);
        let batch = register(&session, &[source]);
        let id = batch.pending[0].0;
        let conn = session.db.lock().unwrap();
        db::assets::set_import_stage(&conn, id, stage).unwrap();
        ids.push(id);
    }
    {
        let conn = session.db.lock().unwrap();
        conn.execute("UPDATE assets SET deleted_at=1 WHERE id=?1", [ids[3]])
            .unwrap();
        assert!(db::assets::recoverable_imports(&conn).unwrap().is_empty());
    }
}

#[test]
fn persisted_file_task_continues_without_duplicating_registered_asset() {
    let (temp, session) = setup_library();
    let first = temp.path().join("first.png");
    let second = temp.path().join("second.png");
    make_image(&first, [180, 60, 30]);
    make_image(&second, [30, 60, 180]);
    let source_strings = vec![
        first.to_string_lossy().into_owned(),
        second.to_string_lossy().into_owned(),
    ];
    {
        let conn = session.db.lock().unwrap();
        db::import_jobs::start(&conn, "file-job", "paths", &source_strings, None).unwrap();
    }
    let first_summary = import::run_import(
        &session,
        std::slice::from_ref(&first),
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(first_summary.done, 1);

    let meta = session.meta.clone();
    drop(session);
    let reopened = Arc::new(library::open_session(meta).unwrap());
    let job = {
        let conn = reopened.db.lock().unwrap();
        db::import_jobs::get(&conn, "file-job").unwrap().unwrap()
    };
    let paths: Vec<_> = job.paths.iter().map(PathBuf::from).collect();
    let resumed = import::run_import(
        &reopened,
        &paths,
        job.folder_id,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(
        resumed.duplicates, 1,
        "Skip registered assets by content hash"
    );
    assert_eq!(resumed.done, 1, "Continue importing unregistered assets");
    let conn = reopened.db.lock().unwrap();
    assert_eq!(
        db::assets::query_assets(
            &conn,
            &reopened.root,
            &db::model::AssetFilter {
                page: 1,
                page_size: 10,
                ..Default::default()
            },
        )
        .unwrap()
        .total,
        2
    );
}

#[test]
fn multi_folder_task_round_trips_and_abandon_keeps_ingested_assets() {
    let (temp, session) = setup_library();
    let source = temp.path().join("already-in-library.png");
    make_image(&source, [70, 100, 130]);
    let summary = import::run_import(
        &session,
        std::slice::from_ref(&source),
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(summary.done, 1);
    let folder_paths = vec!["/source/photos".into(), "/source/videos".into()];
    {
        let conn = session.db.lock().unwrap();
        db::import_jobs::start(&conn, "folder-job", "folders", &folder_paths, None).unwrap();
    }

    let meta = session.meta.clone();
    drop(session);
    let reopened = Arc::new(library::open_session(meta).unwrap());
    {
        let conn = reopened.db.lock().unwrap();
        let job = db::import_jobs::get(&conn, "folder-job").unwrap().unwrap();
        assert_eq!(job.mode, "folders");
        assert_eq!(job.paths, folder_paths);
        db::import_jobs::abandon(&conn, &job.id).unwrap();
        assert!(db::import_jobs::recoverable(&conn).unwrap().is_empty());
        assert_eq!(
            db::assets::query_assets(
                &conn,
                &reopened.root,
                &db::model::AssetFilter {
                    page: 1,
                    page_size: 10,
                    ..Default::default()
                },
            )
            .unwrap()
            .total,
            1,
            "Abandoning remaining work must preserve registered assets"
        );
    }
}

#[test]
fn interrupted_video_rebuilds_thumbnail_and_search_frames() {
    if !clip_studio_lib::util::test_prerequisite(
        clip_studio_lib::media::ffmpeg_available(),
        "FFmpeg unavailable",
    ) {
        return;
    }
    let (temp, session) = setup_library();
    let video = temp.path().join("interrupted.mp4");
    let status = Command::new(clip_studio_lib::media::ffmpeg_path())
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=160x90:rate=24:duration=2",
            "-an",
            "-c:v",
            "mpeg4",
        ])
        .arg(&video)
        .status();
    if !status.is_ok_and(|status| status.success()) {
        if clip_studio_lib::util::test_prerequisite(false, "Could not generate the test video") {
            unreachable!();
        }
        return;
    }

    let mut batch = register(&session, &[video]);
    let id = batch.pending[0].0;
    {
        let conn = session.db.lock().unwrap();
        db::assets::set_import_stage(&conn, id, "frames").unwrap();
    }
    import::process_pending(
        &session,
        &mut batch,
        &AtomicBool::new(false),
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(batch.summary.done, 1);
    let restored = asset(&session, id);
    assert_eq!(restored.import_stage, "embedding");
    assert!(restored
        .thumb_path
        .as_ref()
        .is_some_and(|path| Path::new(path).is_file()));
    let conn = session.db.lock().unwrap();
    assert!(!db::assets::get_frame_ids(&conn, id).unwrap().is_empty());
}

#[test]
fn recovered_asset_becomes_searchable_when_models_are_available() {
    if !clip_studio_lib::util::test_prerequisite(
        clip_studio_lib::clip::resolve_models_dir().is_some(),
        "Complete local model files are unavailable",
    ) {
        return;
    }
    let (temp, session) = setup_library();
    let source = temp.path().join("searchable-recovery.png");
    make_image(&source, [230, 180, 30]);
    let mut batch = register(&session, &[source]);
    let id = batch.pending[0].0;
    import::process_pending(
        &session,
        &mut batch,
        &AtomicBool::new(false),
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(asset(&session, id).import_stage, "embedding");

    let clip = clip_studio_lib::clip::ClipHandle::default()
        .get_or_init()
        .unwrap();
    vector::embed_pending(&session, &clip, &|_| {}).unwrap();
    assert_eq!(asset(&session, id).import_stage, "done");

    let query = clip.encode_texts(&["yellow image"]).unwrap().remove(0);
    let filter = vector::build_filter(&db::model::AssetFilter::default());
    let hits = vector::search(&session, query, filter, 10).unwrap();
    assert!(hits.iter().any(|hit| {
        hit.payload
            .as_ref()
            .and_then(|payload| payload.0.get("asset_id"))
            .and_then(|value| value.as_i64())
            == Some(id)
    }));
}

#[test]
fn duplicate_registration_returns_existing_identity_without_requeueing_media() {
    let (temp, session) = setup_library();
    let source = temp.path().join("same.png");
    make_image(&source, [42, 90, 123]);
    let first = register(&session, std::slice::from_ref(&source));
    assert_eq!(first.pending.len(), 1);
    let id = first.pending[0].0;
    let second = register(&session, &[source]);
    assert!(second.pending.is_empty());
    assert_eq!(second.duplicates, vec![id]);
    assert_eq!(second.summary.duplicates, 1);
    assert_eq!(
        session
            .read_db()
            .query_row("SELECT count(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}
