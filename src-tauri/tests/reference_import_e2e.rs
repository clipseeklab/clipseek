//! End-to-end reference tests cover registration, processing, deduplication, relinking, and
//! deletion safety using public crate APIs only.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clip_studio_lib::db;
use clip_studio_lib::services::assets;
use clip_studio_lib::{import, library};

fn test_png(path: &Path) {
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([20, 80, 160])))
        .save(path)
        .unwrap();
}

fn test_session(name: &str) -> (tempfile::TempDir, Arc<library::LibrarySession>) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(name);
    let meta = library::init_library(&root, name).unwrap();
    let session = Arc::new(library::open_session(meta).unwrap());
    (temp, session)
}

fn register(
    session: &Arc<library::LibrarySession>,
    source: &PathBuf,
    mode: import::ImportMode,
) -> import::RegisteredBatch {
    let cancel = AtomicBool::new(false);
    import::register_files(
        session,
        std::slice::from_ref(source),
        None,
        &cancel,
        mode,
        &|_, _, _| {},
        &|_| {},
    )
}

fn process(session: &Arc<library::LibrarySession>, batch: &mut import::RegisteredBatch) {
    let cancel = AtomicBool::new(false);
    import::process_pending(session, batch, &cancel, &|_, _, _| {}, &|_| {});
}

fn asset(session: &library::LibrarySession, id: i64) -> db::model::Asset {
    let conn = session.db.lock().unwrap();
    db::assets::get_asset(&conn, &session.root, id)
        .unwrap()
        .unwrap()
}

#[test]
fn reference_import_skips_copy_and_processes_from_source() {
    let source_temp = tempfile::tempdir().unwrap();
    let source = source_temp.path().join("photo.png");
    test_png(&source);
    let (_temp, session) = test_session("reference-import");
    let dirs = library::LibraryDirs::new(&session.root);

    let mut batch = register(&session, &source, import::ImportMode::Reference);
    assert_eq!(batch.pending.len(), 1);
    assert_eq!(batch.summary.duplicates, 0);
    let asset_id = batch.pending[0].0;

    {
        let conn = session.db.lock().unwrap();
        let asset = db::assets::get_asset(&conn, &session.root, asset_id)
            .unwrap()
            .unwrap();
        drop(conn);
        assert_eq!(asset.storage, "reference");
        assert_eq!(
            asset.source_path.as_deref(),
            Some(source.to_string_lossy().as_ref())
        );
        assert_eq!(asset.abs_path, source.to_string_lossy());
        // Original files are never copied into the library.
        assert!(!dirs
            .asset(asset.imported_at, &asset.uid)
            .join(&asset.file_name)
            .exists());
    }

    // Processing reads the original source and writes derived files inside the library.
    process(&session, &mut batch);
    let asset = asset(&session, asset_id);
    assert_eq!(asset.import_stage, "embedding");
    assert!(asset.thumb_path.is_some());
    assert!(source.exists(), "source must never be consumed");
}

#[test]
fn missing_reference_source_is_flagged_and_reported() {
    let source_temp = tempfile::tempdir().unwrap();
    let source = source_temp.path().join("photo.png");
    test_png(&source);
    let (_temp, session) = test_session("reference-missing");

    let mut batch = register(&session, &source, import::ImportMode::Reference);
    let asset_id = batch.pending[0].0;
    std::fs::remove_file(&source).unwrap();

    process(&session, &mut batch);
    let asset = asset(&session, asset_id);
    assert!(asset.source_missing);
    assert!(
        asset.import_stage.starts_with("error:"),
        "stage={}",
        asset.import_stage
    );
}

#[test]
fn reimport_relinks_missing_reference_asset() {
    let source_temp = tempfile::tempdir().unwrap();
    let source = source_temp.path().join("photo.png");
    test_png(&source);
    let (_temp, session) = test_session("reference-relink");

    let batch = register(&session, &source, import::ImportMode::Reference);
    let asset_id = batch.pending[0].0;
    let moved = source_temp.path().join("renamed.png");
    std::fs::rename(&source, &moved).unwrap();

    // Relinking must detect a missing old path even before a batch source check updates the
    // database flag.
    let batch = register(&session, &moved, import::ImportMode::Reference);
    assert_eq!(batch.summary.done, 1, "Relinking uses the recovery path");
    assert_eq!(batch.summary.duplicates, 0);

    let asset = asset(&session, asset_id);
    assert_eq!(
        asset.source_path.as_deref(),
        Some(moved.to_string_lossy().as_ref())
    );
    assert!(!asset.source_missing);
    assert_eq!(asset.import_stage, "pending");
    let mtime = std::fs::metadata(&moved)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    assert_eq!(asset.source_mtime, Some(mtime));
}

#[test]
fn duplicate_reference_import_is_skipped() {
    let source_temp = tempfile::tempdir().unwrap();
    let source = source_temp.path().join("photo.png");
    test_png(&source);
    let (_temp, session) = test_session("reference-dup");

    let first = register(&session, &source, import::ImportMode::Reference);
    assert_eq!(first.pending.len(), 1);
    let second = register(&session, &source, import::ImportMode::Reference);
    assert_eq!(second.summary.duplicates, 1);

    let conn = session.db.lock().unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 1, "Repeated content must not create another record");
}

#[test]
fn copy_asset_is_not_relinked_by_reference_reimport() {
    let source_temp = tempfile::tempdir().unwrap();
    let source = source_temp.path().join("photo.png");
    test_png(&source);
    let (_temp, session) = test_session("copy-no-relink");

    // Construct a legacy managed asset and delete its internal copy to simulate a missing
    // file.
    let batch = register(&session, &source, import::ImportMode::Reference);
    let copy_id = batch.pending[0].0;
    if true {
        let conn = session.db.lock().unwrap();
        let rel: String = conn
            .query_row("SELECT rel_path FROM assets WHERE id=?", [copy_id], |r| {
                r.get(0)
            })
            .unwrap();
        let owned = session.root.join(rel);
        std::fs::create_dir_all(owned.parent().unwrap()).unwrap();
        std::fs::copy(&source, owned).unwrap();
        conn.execute(
            "UPDATE assets SET source_path=NULL,source_mtime=NULL WHERE id=?",
            [copy_id],
        )
        .unwrap();
    }

    let copied = asset(&session, copy_id);
    std::fs::remove_file(&copied.abs_path).unwrap();

    // A reference reimport matching a legacy managed asset must not relink that managed
    // record.
    let batch = register(&session, &source, import::ImportMode::Reference);
    assert_eq!(batch.summary.duplicates, 1);

    let asset = asset(&session, copy_id);
    assert_eq!(asset.storage, "copy");
    assert!(
        asset.source_path.is_none(),
        "Do not rewrite legacy managed assets as references"
    );
}

#[test]
fn trash_and_delete_never_touch_reference_source() {
    let source_temp = tempfile::tempdir().unwrap();
    let source = source_temp.path().join("photo.png");
    test_png(&source);
    let (_temp, session) = test_session("reference-delete");

    let batch = register(&session, &source, import::ImportMode::Reference);
    let asset_id = batch.pending[0].0;

    assets::trash_assets(&session, &[asset_id]).unwrap();
    assert!(asset(&session, asset_id).deleted_at.is_some());
    {
        let conn = session.db.lock().unwrap();
        let paths = db::assets::reference_source_paths(&conn).unwrap();
        assert_eq!(paths, vec![source.to_string_lossy().into_owned()]);
    }
    assets::restore_assets(&session, &[asset_id]).unwrap();
    assert!(source.exists(), "restore must not move the user's file");
    assets::delete_permanent(&session, &[asset_id]).unwrap();
    assert!(source.exists(), "delete must never remove the user's file");
}

#[test]
fn canceled_retry_leaves_remaining_assets_unprocessed() {
    let (_temp, session) = test_session("cancel-retry");
    let cancel = AtomicBool::new(true);
    let result = import::reprocess_assets_cancellable(
        &session,
        &[999],
        &|_| {
            panic!("canceled retry must not start processing another asset");
        },
        &cancel,
    );
    assert_eq!(result, (0, 0));
}
