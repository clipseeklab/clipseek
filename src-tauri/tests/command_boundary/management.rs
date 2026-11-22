use super::*;

#[test]
fn library_recovery_forgets_only_the_inactive_registration_through_ipc() {
    let (dir, app, session) = fixture();
    let old = clip_studio_lib::library::init_library(&dir.path().join("old"), "Old").unwrap();
    let config_path = dir.path().join("config.json");
    let mut config = GlobalConfig::load_from(config_path.clone()).unwrap();
    config.libraries = vec![old.clone(), session.meta.clone()];
    config.active_library_id = Some(session.meta.id.clone());
    config.onboarding.completed = true;
    config.save().unwrap();
    *app.state::<AppState>().config.write().unwrap() = config;
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(
        &window,
        "library_delete",
        json!({"id":session.meta.id,"deleteFiles":false})
    )
    .is_err());
    invoke(
        &window,
        "library_delete",
        json!({"id":old.id,"deleteFiles":false}),
    )
    .unwrap();
    let restored = GlobalConfig::load_from(config_path).unwrap();
    assert_eq!(restored.libraries.len(), 1);
    assert_eq!(
        restored.active_library_id.as_deref(),
        Some(session.meta.id.as_str())
    );
    assert!(restored.onboarding.completed);
    assert!(std::path::Path::new(&old.path)
        .join(".clipstudio-open/library.db")
        .is_file());
    assert_eq!(
        app.state::<AppState>()
            .session
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .meta
            .id,
        session.meta.id
    );
}

#[test]
fn bulk_queue_clear_is_library_scoped_and_keeps_active_materials_and_assets() {
    let (_dir, app, session) = fixture();
    let running = asset(&session, None, "running.jpg");
    let done = asset(&session, None, "done.jpg");
    {
        let conn = session.db.lock().unwrap();
        for id in [running, done] {
            clip_studio_lib::db::indexing::enqueue(&conn, id, "visual").unwrap();
        }
        let task: i64 = conn
            .query_row("SELECT id FROM index_tasks WHERE asset_id=?", [done], |r| {
                r.get(0)
            })
            .unwrap();
        clip_studio_lib::db::indexing::set_status(
            &conn,
            Some(task),
            "completed",
            None,
            &["queued", "running"],
        )
        .unwrap();
        // More than a page, including failures without an asset ID.
        for i in 0..25 {
            clip_studio_lib::db::processing::register(
                &conn,
                "bulk",
                &std::path::PathBuf::from(format!("/missing/{i}.png")),
                None,
            )
            .unwrap();
        }
        clip_studio_lib::db::processing::finish_batch(&conn, "bulk", "failed", Some("missing"))
            .unwrap();
    }
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(
        &window,
        "processing_clear",
        json!({"libraryId":"other-library"})
    )
    .is_err());
    let before = invoke(
        &window,
        "processing_list",
        json!({"libraryId":session.meta.id,"filter":{}}),
    )
    .unwrap();
    assert_eq!(before["total"], 27);
    assert_eq!(
        invoke(
            &window,
            "processing_clear",
            json!({"libraryId":session.meta.id})
        )
        .unwrap(),
        json!(26)
    );
    let after = invoke(
        &window,
        "processing_list",
        json!({"libraryId":session.meta.id,"filter":{}}),
    )
    .unwrap();
    assert_eq!(after["total"], 1);
    assert_eq!(after["items"][0]["assetId"], running);
    assert_eq!(
        invoke(
            &window,
            "processing_clear",
            json!({"libraryId":session.meta.id})
        )
        .unwrap(),
        json!(0)
    );
    let conn = session.read_db();
    assert!(
        clip_studio_lib::db::assets::get_asset(&conn, &session.root, done)
            .unwrap()
            .is_some()
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM processing_tasks", [], |r| r
            .get::<_, i64>(0))
            .unwrap(),
        27
    );
    assert_eq!(
        conn.query_row(
            "SELECT status FROM index_tasks WHERE asset_id=?",
            [running],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "queued"
    );
}

#[test]
fn durable_queue_ipc_rejects_stale_library_actions_and_preserves_active_work() {
    let (_dir, app, session) = fixture();
    let id = asset(&session, None, "one.jpg");
    clip_studio_lib::db::indexing::enqueue(&session.db.lock().unwrap(), id, "visual").unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let result=invoke(&window,"processing_list",json!({"libraryId":session.meta.id,"filter":{"status":"processing","page":1,"pageSize":10}})).unwrap();
    assert_eq!(result["total"], 1);
    let key = result["items"][0]["key"].as_str().unwrap();
    assert!(invoke(
        &window,
        "processing_hide",
        json!({"libraryId":session.meta.id,"key":key})
    )
    .is_err());
    assert!(invoke(
        &window,
        "processing_list",
        json!({"libraryId":"other-library","filter":{}})
    )
    .is_err());
    assert!(invoke(
        &window,
        "processing_summary",
        json!({"libraryId":"other-library"})
    )
    .is_err());
    assert!(invoke(
        &window,
        "processing_history",
        json!({"libraryId":"other-library","key":key})
    )
    .is_err());
    let summary = invoke(
        &window,
        "processing_summary",
        json!({"libraryId":session.meta.id}),
    )
    .unwrap();
    assert_eq!(summary["activeCount"], 1);
    let history = invoke(
        &window,
        "processing_history",
        json!({"libraryId":session.meta.id,"key":key}),
    )
    .unwrap();
    assert_eq!(history["tasks"].as_array().unwrap().len(), 1);
    assert!(invoke(
        &window,
        "processing_hide",
        json!({"libraryId":"other-library","key":key})
    )
    .is_err());
    clip_studio_lib::db::indexing::set_status(
        &session.db.lock().unwrap(),
        None,
        "completed",
        None,
        &["queued", "running"],
    )
    .unwrap();
    invoke(
        &window,
        "processing_hide",
        json!({"libraryId":session.meta.id,"key":key}),
    )
    .unwrap();
    let result = invoke(
        &window,
        "processing_list",
        json!({"libraryId":session.meta.id,"filter":{}}),
    )
    .unwrap();
    assert_eq!(result["total"], 0);
    assert!(
        clip_studio_lib::db::assets::get_asset(&session.read_db(), &session.root, id)
            .unwrap()
            .is_some()
    );
}

fn fixture() -> (
    tempfile::TempDir,
    tauri::App<MockRuntime>,
    Arc<LibrarySession>,
) {
    let dir = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&dir.path().join("library"), "Regression library")
            .unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    let app = test_app();
    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    (dir, app, session)
}
fn asset(session: &LibrarySession, folder: Option<i64>, name: &str) -> i64 {
    let conn = session.db.lock().unwrap();
    clip_studio_lib::db::assets::insert_asset(
        &conn,
        clip_studio_lib::db::assets::NewAsset {
            folder_id: folder,
            file_name: name,
            rel_path: name,
            file_hash: name,
            file_size: 3,
            mime_type: "image/jpeg",
            kind: "image",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap()
}
fn tags_for(session: &LibrarySession, id: i64) -> Vec<String> {
    let conn = session.read_db();
    let mut stmt = conn.prepare("SELECT t.name FROM tags t JOIN asset_tags a ON a.tag_id=t.id WHERE a.asset_id=? ORDER BY t.name").unwrap();
    stmt.query_map([id], |r| r.get(0))
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap()
}

#[test]
fn tag_move_merges_overlaps_preserves_other_tags_and_never_deletes_assets() {
    let (_dir, app, session) = fixture();
    let one = asset(&session, None, "one.jpg");
    let two = asset(&session, None, "two.jpg");
    {
        let conn = session.db.lock().unwrap();
        clip_studio_lib::db::tags::add_to_asset(
            &conn,
            one,
            &["A".into(), "B".into(), "Target".into(), "Keep".into()],
        )
        .unwrap();
        clip_studio_lib::db::tags::add_to_asset(&conn, two, &["B".into(), "Keep".into()]).unwrap();
    }
    let list = clip_studio_lib::db::tags::list(&session.read_db()).unwrap();
    let id = |name: &str| list.iter().find(|t| t.name == name).unwrap().id;
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    invoke(
        &webview,
        "tag_batch",
        json!({"ids":[id("A"),id("B"),id("A")],"targetId":id("Target")}),
    )
    .unwrap();
    assert_eq!(tags_for(&session, one), vec!["Keep", "Target"]);
    assert_eq!(tags_for(&session, two), vec!["Keep", "Target"]);
    let counts = clip_studio_lib::db::tags::list(&session.read_db()).unwrap();
    assert_eq!(
        counts.iter().find(|t| t.name == "A").unwrap().asset_count,
        0
    );
    assert_eq!(
        counts.iter().find(|t| t.name == "B").unwrap().asset_count,
        0
    );
    assert_eq!(
        counts
            .iter()
            .find(|t| t.name == "Target")
            .unwrap()
            .asset_count,
        2
    );
    invoke(
        &webview,
        "tag_batch",
        json!({"ids":[id("Target")],"targetId":null}),
    )
    .unwrap();
    assert_eq!(tags_for(&session, one), vec!["Keep"]);
    assert_eq!(tags_for(&session, two), vec!["Keep"]);
    let conn = session.read_db();
    assert_eq!(
        conn.query_row(
            "SELECT count(*) FROM assets WHERE deleted_at IS NULL",
            [],
            |r| r.get::<_, i64>(0)
        )
        .unwrap(),
        2
    );
}
#[test]
fn tag_target_validation_is_atomic_and_literal_search_does_not_treat_percent_as_wildcard() {
    let (_dir, app, session) = fixture();
    let one = asset(&session, None, "one.jpg");
    {
        let conn = session.db.lock().unwrap();
        clip_studio_lib::db::tags::add_to_asset(&conn, one, &["100%旅行".into(), "Other".into()])
            .unwrap();
    }
    let list = clip_studio_lib::db::tags::list(&session.read_db()).unwrap();
    let id = list[0].id;
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    for body in [
        json!({"ids":[id],"targetId":999999}),
        json!({"ids":[id],"targetId":id}),
        json!({"ids":[],"targetId":null}),
    ] {
        assert!(invoke(&webview, "tag_batch", body).is_err());
    }
    assert_eq!(tags_for(&session, one).len(), 2);
    let page = invoke(
        &webview,
        "target_options",
        json!({"kind":"tags","query":"%","after":0,"exclude":[]}),
    )
    .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    let matched = page["items"][0]["id"].as_i64().unwrap();
    let page = invoke(
        &webview,
        "target_options",
        json!({"kind":"tags","query":"%","after":0,"exclude":[matched]}),
    )
    .unwrap();
    assert!(page["items"].as_array().unwrap().is_empty());
    assert!(invoke(
        &webview,
        "target_options",
        json!({"kind":"assets; DROP TABLE tags","query":"","after":0,"exclude":[]})
    )
    .is_err());
    assert_eq!(
        clip_studio_lib::db::tags::list(&session.read_db())
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn target_pagination_covers_all_rows_without_duplicates_and_has_an_exact_final_page() {
    let (_dir, app, _session) = fixture();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let names = (0..120).map(|i| format!("Target {i}")).collect::<Vec<_>>();
    invoke(&webview, "tag_create_many", json!({"names":names})).unwrap();
    let mut cursor = 0;
    let mut ids = std::collections::HashSet::new();
    for expected in [50, 50, 20] {
        let page = invoke(
            &webview,
            "target_options",
            json!({"kind":"tags","query":"target","after":cursor,"exclude":[]}),
        )
        .unwrap();
        let rows = page["items"].as_array().unwrap();
        assert_eq!(rows.len(), expected);
        for row in rows {
            let id = row["id"].as_i64().unwrap();
            assert!(id > cursor);
            assert!(ids.insert(id));
        }
        cursor = rows.last().unwrap()["id"].as_i64().unwrap();
        assert_eq!(page["hasMore"], json!(expected == 50));
    }
    assert_eq!(ids.len(), 120);
}
#[test]
fn folder_move_and_delete_preserve_reference_sources_and_unrelated_assets() {
    let (dir, app, session) = fixture();
    let a = clip_studio_lib::services::folders::create_folder(&session, "A").unwrap();
    let b = clip_studio_lib::services::folders::create_folder(&session, "B").unwrap();
    let target = clip_studio_lib::services::folders::create_folder(&session, "Target").unwrap();
    let source = dir.path().join("external.jpg");
    std::fs::write(&source, b"external-original").unwrap();
    let one = {
        let conn = session.db.lock().unwrap();
        clip_studio_lib::db::assets::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: Some(a.id),
                file_name: "ref.jpg",
                rel_path: "ref.jpg",
                file_hash: "ref-hash",
                file_size: 17,
                mime_type: "image/jpeg",
                kind: "image",
                imported_at: 1,
                uid: Some("ref-id"),
                source: Some(clip_studio_lib::db::assets::ReferenceSource {
                    path: source.to_str().unwrap(),
                    modified_at: 1,
                }),
            },
        )
        .unwrap()
    };
    let unrelated = asset(&session, None, "unrelated.jpg");
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(
        &webview,
        "folder_batch",
        json!({"ids":[a.id,b.id],"targetId":999999})
    )
    .is_err());
    assert_eq!(
        clip_studio_lib::db::assets::get_asset(&session.read_db(), &session.root, one)
            .unwrap()
            .unwrap()
            .folder_id,
        Some(a.id)
    );
    invoke(
        &webview,
        "folder_batch",
        json!({"ids":[a.id,b.id],"targetId":target.id}),
    )
    .unwrap();
    assert_eq!(
        clip_studio_lib::db::assets::get_asset(&session.read_db(), &session.root, one)
            .unwrap()
            .unwrap()
            .folder_id,
        Some(target.id)
    );
    assert_eq!(
        clip_studio_lib::db::folders::list(&session.read_db())
            .unwrap()
            .len(),
        3
    );
    invoke(
        &webview,
        "folder_batch",
        json!({"ids":[target.id],"targetId":null}),
    )
    .unwrap();
    let deleted = clip_studio_lib::db::assets::get_asset(&session.read_db(), &session.root, one)
        .unwrap()
        .unwrap();
    assert!(deleted.deleted_at.is_some());
    assert_eq!(deleted.folder_id, None);
    assert!(
        clip_studio_lib::db::assets::get_asset(&session.read_db(), &session.root, unrelated)
            .unwrap()
            .unwrap()
            .deleted_at
            .is_none()
    );
    assert_eq!(std::fs::read(&source).unwrap(), b"external-original");
}

#[test]
fn bulk_tag_creation_rolls_back_earlier_names_when_a_later_insert_fails() {
    let (_dir, app, session) = fixture();
    session.db.lock().unwrap().execute_batch(
        "CREATE TRIGGER fail_tag_insert BEFORE INSERT ON tags WHEN NEW.name='Rejected' BEGIN SELECT RAISE(ABORT,'test insert failure'); END;"
    ).unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let result = invoke(
        &webview,
        "tag_create_many",
        json!({"names":["First", "Rejected", "Last"]}),
    );
    assert!(result.is_err());
    assert!(clip_studio_lib::db::tags::list(&session.read_db())
        .unwrap()
        .is_empty());
    session
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_tag_insert")
        .unwrap();
    assert_eq!(
        invoke(
            &webview,
            "tag_create_many",
            json!({"names":["First", "Rejected", "Last"]})
        )
        .unwrap(),
        json!(3)
    );
}

#[test]
fn tag_move_rolls_back_all_links_when_a_later_source_fails() {
    let (_dir, app, session) = fixture();
    let one = asset(&session, None, "one.jpg");
    let two = asset(&session, None, "two.jpg");
    {
        let conn = session.db.lock().unwrap();
        clip_studio_lib::db::tags::add_to_asset(&conn, one, &["A".into(), "Keep".into()]).unwrap();
        clip_studio_lib::db::tags::add_to_asset(&conn, two, &["B".into(), "Target".into()])
            .unwrap();
    }
    let tags = clip_studio_lib::db::tags::list(&session.read_db()).unwrap();
    let id = |name: &str| tags.iter().find(|t| t.name == name).unwrap().id;
    session.db.lock().unwrap().execute_batch(&format!(
        "CREATE TRIGGER fail_tag_unlink BEFORE DELETE ON asset_tags WHEN OLD.tag_id={} BEGIN SELECT RAISE(ABORT,'test unlink failure'); END;", id("B")
    )).unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(
        &webview,
        "tag_batch",
        json!({"ids":[id("A"),id("B")],"targetId":id("Target")})
    )
    .is_err());
    assert_eq!(tags_for(&session, one), vec!["A", "Keep"]);
    assert_eq!(tags_for(&session, two), vec!["B", "Target"]);
    assert_eq!(
        clip_studio_lib::db::tags::list(&session.read_db())
            .unwrap()
            .len(),
        tags.len()
    );
    session
        .db
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_tag_unlink")
        .unwrap();
    invoke(
        &webview,
        "tag_batch",
        json!({"ids":[id("A"),id("B")],"targetId":id("Target")}),
    )
    .unwrap();
    assert_eq!(tags_for(&session, one), vec!["Keep", "Target"]);
    assert_eq!(tags_for(&session, two), vec!["Target"]);
}

#[test]
fn asset_lists_honor_explicit_library_context_without_changing_selection() {
    let (root, app, active) = fixture();
    let meta = clip_studio_lib::library::init_library(&root.path().join("other"), "Other").unwrap();
    let other = Arc::new(clip_studio_lib::library::open_session_light(meta).unwrap());
    asset(&active, None, "active.jpg");
    asset(&other, None, "other.jpg");
    {
        let state = app.state::<AppState>();
        let mut config = state.config.write().unwrap();
        config.libraries = vec![active.meta.clone(), other.meta.clone()];
        config.active_library_id = Some(active.meta.id.clone());
        state
            .session_cache
            .lock()
            .unwrap()
            .insert(other.meta.id.clone(), Arc::downgrade(&other));
    }
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let mut headers = tauri::http::HeaderMap::new();
    headers.insert("x-clipseek-library", other.meta.id.parse().unwrap());
    let result = invoke_with_headers(&window, "asset_list", json!({"filter":{}}), headers).unwrap();
    assert_eq!(result["total"], 1);
    assert_eq!(result["items"][0]["fileName"], "other.jpg");
    let unscoped = invoke(&window, "asset_list", json!({"filter":{}})).unwrap();
    assert_eq!(unscoped["items"][0]["fileName"], "active.jpg");
    assert_eq!(
        app.state::<AppState>()
            .session
            .read()
            .unwrap()
            .as_ref()
            .unwrap()
            .meta
            .id,
        active.meta.id
    );
    let mut invalid = tauri::http::HeaderMap::new();
    invalid.insert("x-clipseek-library", "unknown-library".parse().unwrap());
    let error =
        invoke_with_headers(&window, "asset_list", json!({"filter":{}}), invalid).unwrap_err();
    assert_error_code(&error, "library_not_found");
}
