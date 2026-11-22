//! End-to-end reference-source checks cover flag maintenance and cancellation.

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clip_studio_lib::commands::assets::scan_reference_sources;
use clip_studio_lib::db;
use clip_studio_lib::library;

fn test_session(name: &str) -> Arc<library::LibrarySession> {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join(name);
    let meta = library::init_library(&root, name).unwrap();
    Arc::new(library::open_session(meta).unwrap())
}

fn insert_reference(session: &library::LibrarySession, source: &Path, missing: bool) -> i64 {
    let conn = session.db.lock().unwrap();
    let id = db::assets::insert_asset(
        &conn,
        clip_studio_lib::db::assets::NewAsset {
            folder_id: None,
            file_name: "photo.png",
            rel_path: "images/00-01/x/photo.png",
            file_hash: &clip_studio_lib::util::new_entity_uid(),
            file_size: 128,
            mime_type: "image/png",
            kind: "image",
            imported_at: 1_700_000_000,
            uid: Some(&clip_studio_lib::util::new_entity_uid()),
            source: Some(clip_studio_lib::db::assets::ReferenceSource {
                path: &source.to_string_lossy(),
                modified_at: 1_700_000_000,
            }),
        },
    )
    .unwrap();
    db::assets::set_source_missing(&conn, id, missing).unwrap();
    id
}

fn source_missing(session: &library::LibrarySession, id: i64) -> bool {
    asset(session, id).source_missing
}

fn asset(session: &library::LibrarySession, id: i64) -> db::model::Asset {
    let conn = session.db.lock().unwrap();
    db::assets::get_asset(&conn, &session.root, id)
        .unwrap()
        .unwrap()
}

#[test]
fn scan_flags_missing_sources_and_clears_stale_flags() {
    let dir = tempfile::tempdir().unwrap();
    let alive = dir.path().join("alive.png");
    std::fs::write(&alive, b"png").unwrap();
    let gone = dir.path().join("gone.png");

    let session = test_session("scan-flags");
    let alive_id = insert_reference(&session, &alive, true);
    let gone_id = insert_reference(&session, &gone, false);

    let summary = scan_reference_sources(&session, &AtomicBool::new(false), &|_, _| {}).unwrap();
    assert_eq!(summary.total, 2);
    assert_eq!(summary.missing, 1);
    assert!(!summary.cancelled);

    // Clear stale flags for live sources and mark absent sources missing.
    assert!(!source_missing(&session, alive_id));
    assert!(source_missing(&session, gone_id));
}

#[test]
fn scan_can_be_cancelled_before_work() {
    let session = test_session("scan-cancel");
    let missing = tempfile::tempdir().unwrap();
    let id = insert_reference(&session, &missing.path().join("none.png"), false);

    let cancel = AtomicBool::new(true);
    let summary = scan_reference_sources(&session, &cancel, &|_, _| {}).unwrap();
    assert!(summary.cancelled);
    assert_eq!(summary.total, 1);
    assert_eq!(summary.missing, 0, "Cancellation must not update any flags");
    assert!(!source_missing(&session, id));

    let conn = session.db.lock().unwrap();
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM assets WHERE source_missing = 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
}
