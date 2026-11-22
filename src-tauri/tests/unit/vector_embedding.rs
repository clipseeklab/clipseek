use super::*;

#[test]
fn a_batch_finishing_after_deletion_intent_cannot_publish_readiness() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Embedding cancellation").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    let conn = session.db.lock().unwrap();
    let id = db::assets::insert_asset(
        &conn,
        crate::db::assets::NewAsset {
            folder_id: None,
            file_name: "source.png",
            rel_path: "source.png",
            file_hash: "hash",
            file_size: 1,
            mime_type: "image/png",
            kind: "image",
            imported_at: 1,
            uid: Some("embedding-deletion"),
            ..Default::default()
        },
    )
    .unwrap();
    db::assets::insert_frame(&conn, id, 0, 0, Some("source.png")).unwrap();
    db::assets::set_import_stage(&conn, id, "embedding").unwrap();
    let active_id = db::assets::insert_asset(
        &conn,
        crate::db::assets::NewAsset {
            folder_id: None,
            file_name: "active.png",
            rel_path: "active.png",
            file_hash: "active-hash",
            file_size: 1,
            mime_type: "image/png",
            kind: "image",
            imported_at: 1,
            uid: Some("embedding-active"),
            ..Default::default()
        },
    )
    .unwrap();
    db::assets::insert_frame(&conn, active_id, 0, 0, Some("active.png")).unwrap();
    db::assets::set_import_stage(&conn, active_id, "embedding").unwrap();
    let rows = db::assets::pending_frames_full(&conn, temp.path(), 64).unwrap();
    assert_eq!(rows.len(), 2);
    db::asset_cleanup::mark(&conn, "embedding-deletion").unwrap();
    db::assets::set_deleted(&conn, &[id], Some(2)).unwrap();
    drop(conn);

    persist_embedded_frames(&session, &rows).unwrap();

    let asset = db::assets::get_asset(&session.read_db(), temp.path(), id)
        .unwrap()
        .unwrap();
    let active = db::assets::get_asset(&session.read_db(), temp.path(), active_id)
        .unwrap()
        .unwrap();
    assert!(
        active.visual_indexed,
        "the same batch still completes active assets"
    );
    assert_eq!(active.import_stage, "done");
    assert!(asset.deleted_at.is_some());
    assert!(
        !asset.visual_indexed,
        "a stale encoded batch cannot undo deletion readiness"
    );
    assert_eq!(
        asset.import_stage, "embedding",
        "a deleted asset must not be announced as done"
    );
}
