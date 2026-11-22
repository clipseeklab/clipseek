use super::*;
use clip_studio_lib::db::assets as dao;

fn fixture() -> (
    tempfile::TempDir,
    tauri::App<MockRuntime>,
    Arc<LibrarySession>,
    i64,
    Vec<i64>,
) {
    let dir = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&dir.path().join("library"), "Cover test").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    let app = test_app();
    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    let conn = session.db.lock().unwrap();
    let id = dao::insert_asset(
        &conn,
        clip_studio_lib::db::assets::NewAsset {
            folder_id: None,
            file_name: "video.mp4",
            rel_path: "video.mp4",
            file_hash: "cover-video",
            file_size: 5,
            mime_type: "video/mp4",
            kind: "video",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap();
    std::fs::write(session.root.join("video.mp4"), b"video").unwrap();
    image::RgbImage::new(4, 4)
        .save(session.root.join("old.png"))
        .unwrap();
    dao::update_asset_probe(
        &conn,
        id,
        clip_studio_lib::db::assets::AssetProbe {
            width: Some(4),
            height: Some(4),
            duration_ms: Some(12000),
            color: None,
            thumb_rel: Some("old.png"),
        },
    )
    .unwrap();
    let frames = (0..12)
        .map(|seq| {
            let rel = format!("frame-{seq}.png");
            image::RgbImage::new(4, 4)
                .save(session.root.join(&rel))
                .unwrap();
            dao::insert_frame(&conn, id, seq, seq * 1000, Some(&rel)).unwrap()
        })
        .collect();
    drop(conn);
    (dir, app, session, id, frames)
}

#[test]
fn cover_selection_persists_and_preserves_reference_source_and_keyframes() {
    let (dir, app, session, id, frames) = fixture();
    let source = dir.path().join("external.mp4");
    std::fs::write(&source, b"external video").unwrap();
    session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET source_path=?1 WHERE id=?2",
            rusqlite::params![source.to_str().unwrap(), id],
        )
        .unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let result = invoke(
        &webview,
        "asset_set_cover",
        json!({"id":id,"frameId":frames[9]}),
    )
    .unwrap();
    let expected = session
        .root
        .join("frame-9.png")
        .to_string_lossy()
        .into_owned();
    assert_eq!(result["thumbPath"], expected);
    let detail = dao::get_asset_detail(&session.read_db(), &session.root, id)
        .unwrap()
        .unwrap();
    assert_eq!(detail.asset.thumb_path.as_deref(), Some(expected.as_str()));
    assert_eq!(detail.frames.len(), 12);
    for seq in 0..12 {
        assert!(session.root.join(format!("frame-{seq}.png")).is_file());
    }
    assert_eq!(std::fs::read(&source).unwrap(), b"external video");
    assert_eq!(
        std::fs::read(session.root.join("video.mp4")).unwrap(),
        b"video"
    );
    let root = session.root.clone();
    drop(webview);
    drop(app);
    drop(session);
    let reopened = clip_studio_lib::db::open_library_db(
        &clip_studio_lib::library::LibraryDirs::new(&root).db(),
    )
    .unwrap();
    assert_eq!(
        dao::get_asset(&reopened, &root, id)
            .unwrap()
            .unwrap()
            .thumb_path
            .as_deref(),
        Some(expected.as_str())
    );
}

#[test]
fn cover_rejects_frames_outside_first_ten_and_other_assets() {
    let (_dir, app, session, id, frames) = fixture();
    let foreign = {
        let conn = session.db.lock().unwrap();
        let other = dao::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: None,
                file_name: "other.mp4",
                rel_path: "other.mp4",
                file_hash: "other",
                file_size: 5,
                mime_type: "video/mp4",
                kind: "video",
                imported_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
        dao::insert_frame(&conn, other, 0, 0, Some("old.png")).unwrap()
    };
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for frame in [frames[10], frames[11], foreign, i64::MAX] {
        assert_error_code(
            &invoke(
                &webview,
                "asset_set_cover",
                json!({"id":id,"frameId":frame}),
            )
            .unwrap_err(),
            "invalid_argument",
        );
    }
    assert!(dao::get_asset(&session.read_db(), &session.root, id)
        .unwrap()
        .unwrap()
        .thumb_path
        .unwrap()
        .ends_with("old.png"));
    session
        .db
        .lock()
        .unwrap()
        .execute("UPDATE assets SET kind='image' WHERE id=?1", [id])
        .unwrap();
    assert_error_code(
        &invoke(
            &webview,
            "asset_set_cover",
            json!({"id":id,"frameId":frames[0]}),
        )
        .unwrap_err(),
        "asset_type_unsupported",
    );
    session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET kind='video', deleted_at=1 WHERE id=?1",
            [id],
        )
        .unwrap();
    assert_error_code(
        &invoke(
            &webview,
            "asset_set_cover",
            json!({"id":id,"frameId":frames[0]}),
        )
        .unwrap_err(),
        "asset_not_found",
    );
}

#[test]
fn cover_failure_preserves_previous_cover_and_never_uses_external_paths() {
    let (dir, app, session, id, frames) = fixture();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let outside = dir.path().join("outside.png");
    image::RgbImage::new(4, 4).save(&outside).unwrap();
    for rel in [
        None,
        Some("missing.png".into()),
        Some("bad.png".into()),
        Some(outside.to_string_lossy().into_owned()),
    ] {
        std::fs::write(session.root.join("bad.png"), b"invalid image").unwrap();
        session
            .db
            .lock()
            .unwrap()
            .execute(
                "UPDATE frames SET rel_path=?1 WHERE id=?2",
                rusqlite::params![rel, frames[0]],
            )
            .unwrap();
        assert_error_code(
            &invoke(
                &webview,
                "asset_set_cover",
                json!({"id":id,"frameId":frames[0]}),
            )
            .unwrap_err(),
            "cover_frame_unavailable",
        );
        assert!(dao::get_asset(&session.read_db(), &session.root, id)
            .unwrap()
            .unwrap()
            .thumb_path
            .unwrap()
            .ends_with("old.png"));
    }
    *app.state::<AppState>().session.write().unwrap() = None;
    assert_error_code(
        &invoke(
            &webview,
            "asset_set_cover",
            json!({"id":id,"frameId":frames[0]}),
        )
        .unwrap_err(),
        "library_not_open",
    );
}
