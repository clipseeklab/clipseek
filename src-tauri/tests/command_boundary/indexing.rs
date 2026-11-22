use super::*;
fn fixture() -> (
    tempfile::TempDir,
    tauri::App<MockRuntime>,
    Arc<LibrarySession>,
    i64,
    tauri::WebviewWindow<MockRuntime>,
) {
    let temp = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&temp.path().join("library"), "Queue").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session_light(meta).unwrap());
    let app = test_app();
    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let file = session.root.join("movie.mp4");
    std::fs::write(&file, b"metadata-only fixture").unwrap();
    let id = clip_studio_lib::db::assets::insert_asset(
        &session.db.lock().unwrap(),
        clip_studio_lib::db::assets::NewAsset {
            folder_id: None,
            file_name: "movie.mp4",
            rel_path: "movie.mp4",
            file_hash: "movie",
            file_size: 21,
            mime_type: "video/mp4",
            kind: "video",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap();
    session.db.lock().unwrap().execute("UPDATE assets SET import_stage='done',visual_requested=0,subtitle_requested=0 WHERE id=?",[id]).unwrap();
    (temp, app, session, id, webview)
}
#[test]
fn index_summary_is_model_free_and_subtitles_cannot_be_scheduled() {
    let (_temp, _app, _s, id, webview) = fixture();
    let summary = invoke(&webview, "index_summary", json!({})).unwrap();
    assert_eq!(summary["visualModelReady"], json!(false));
    assert_eq!(summary["assets"], json!(1));
    assert!(invoke(
        &webview,
        "index_build",
        json!({"scope":{"assetIds":[id]},"kinds":["subtitles"]})
    )
    .is_err());
}
