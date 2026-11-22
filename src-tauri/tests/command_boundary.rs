//! Test the real command boundary through mock-runtime IPC deserialization, State injection,
//! and AppError serialization rather than calling internal services.

use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc, Mutex, RwLock, Weak};

use clip_studio_lib::commands::{
    assets, folders, import as import_commands, library, settings, tags,
};
use clip_studio_lib::config::GlobalConfig;
use clip_studio_lib::library::LibrarySession;
use clip_studio_lib::{clip, import, AppState};
use serde_json::{json, Value};
use tauri::ipc::{CallbackFn, InvokeBody};
use tauri::test::{
    get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY,
};
use tauri::webview::InvokeRequest;
use tauri::Manager;

fn test_state() -> AppState {
    AppState {
        mcp: Default::default(),
        config: RwLock::new(GlobalConfig::default()),
        session: RwLock::new(None),
        session_cache: Mutex::new(HashMap::<String, Weak<LibrarySession>>::new()),
        session_switch_lock: Mutex::new(()),
        import_ctl: import::ImportControl::default(),
        clip: clip::ClipHandle::default(),
        operation_running: AtomicBool::new(false),
        source_check_running: Arc::new(AtomicBool::new(false)),
        source_check_cancel: Arc::new(AtomicBool::new(false)),
    }
}

fn test_app() -> tauri::App<MockRuntime> {
    mock_builder()
        .manage(test_state())
        .invoke_handler(tauri::generate_handler![
            clip_studio_lib::commands::diagnostics::diagnostics_export,
            clip_studio_lib::commands::indexing::index_build,
            clip_studio_lib::commands::indexing::index_summary,
            clip_studio_lib::commands::indexing::index_tasks,
            clip_studio_lib::commands::indexing::index_task_cancel,
            clip_studio_lib::commands::indexing::index_task_retry,
            clip_studio_lib::commands::processing::processing_list,
            clip_studio_lib::commands::processing::processing_summary,
            clip_studio_lib::commands::processing::processing_history,
            clip_studio_lib::commands::processing::processing_hide,
            clip_studio_lib::commands::processing::processing_clear,
            clip_studio_lib::commands::processing::processing_cancel,
            settings::mcp_status,
            settings::mcp_token_reveal,
            settings::mcp_token_rotate,
            settings::settings_get,
            settings::settings_set,
            settings::onboarding_set,
            settings::onboarding_get,
            library::library_current,
            library::library_rename,
            library::library_delete,
            folders::folder_create,
            folders::folder_page,
            folders::folder_get,
            folders::folder_sidebar,
            folders::folder_sidebar_set,
            folders::folder_pin,
            folders::folder_batch,
            folders::folder_rename,
            tags::tag_rename,
            tags::tag_create_many,
            tags::tag_batch,
            tags::target_options,
            clip_studio_lib::commands::search::search_text,
            assets::asset_list,
            assets::asset_move_to_folder,
            assets::asset_trash,
            assets::asset_playback_source,
            assets::asset_playback_codec,
            assets::asset_export,
            assets::asset_preview,
            assets::asset_set_cover,
            assets::asset_add_tags,
            import_commands::import_retry,
            import_commands::import_recovery_list,
        ])
        .build(mock_context(noop_assets()))
        .expect("mock Tauri app should build")
}

// A held lock represents a slow worker. Dispatch must return before that worker
// releases the lock; the fallback prevents the old synchronous command deadlocking the test.
fn dispatch_while_locked(command: &str, body: Value, config_lock: bool) {
    use std::sync::mpsc;
    use std::time::Duration;
    let app = test_app();
    let temp = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&temp.path().join("library"), "Test").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let locker_app = app.handle().clone();
    let (ready_tx, ready_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let locker = std::thread::spawn(move || {
        let state = locker_app.state::<AppState>();
        let _config = config_lock.then(|| state.config.write().unwrap());
        let _db = (!config_lock).then(|| session.db.lock().unwrap());
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(2)).is_ok()
    });
    ready_rx.recv().unwrap();
    let (response_tx, response_rx) = mpsc::channel();
    webview.as_ref().clone().on_message(
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(body),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
        Box::new(move |_, _, response, _, _| {
            let _ = response_tx.send(response);
        }),
    );
    let _ = release_tx.send(());
    let dispatched_before_release = locker.join().unwrap();
    response_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("command should complete after releasing the lock");
    assert!(
        dispatched_before_release,
        "{command} blocked IPC dispatch while waiting for a worker lock"
    );
}

#[test]
fn export_does_not_block_ipc_dispatch_on_database_lock() {
    let target = tempfile::tempdir().unwrap();
    dispatch_while_locked(
        "asset_export",
        json!({ "ids": [999], "targetDir": target.path().to_string_lossy() }),
        false,
    );
}

struct ExportFixture {
    _app: tauri::App<MockRuntime>,
    webview: tauri::WebviewWindow<MockRuntime>,
    temp: tempfile::TempDir,
    session: Arc<LibrarySession>,
    asset_id: i64,
    source: std::path::PathBuf,
    export_dir: std::path::PathBuf,
}

fn export_fixture() -> ExportFixture {
    export_fixture_with_mode(import::ImportMode::Reference)
}

fn export_fixture_with_mode(mode: import::ImportMode) -> ExportFixture {
    let app = test_app();
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let meta = clip_studio_lib::library::init_library(&root, "Export safety").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    let source = temp.path().join("photo.png");
    image::DynamicImage::new_rgb8(8, 8).save(&source).unwrap();
    let batch = import::register_files(
        &session,
        std::slice::from_ref(&source),
        None,
        &AtomicBool::new(false),
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(batch.pending.len(), 1);
    let asset_id = batch.pending[0].0;
    if mode == import::ImportMode::Copy {
        let conn = session.db.lock().unwrap();
        let rel: String = conn
            .query_row("SELECT rel_path FROM assets WHERE id=?", [asset_id], |r| {
                r.get(0)
            })
            .unwrap();
        let owned = root.join(rel);
        std::fs::create_dir_all(owned.parent().unwrap()).unwrap();
        std::fs::copy(&source, owned).unwrap();
        conn.execute(
            "UPDATE assets SET source_path=NULL,source_mtime=NULL WHERE id=?",
            [asset_id],
        )
        .unwrap();
    }

    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let export_dir = temp.path().join("export");
    std::fs::create_dir(&export_dir).unwrap();
    ExportFixture {
        _app: app,
        webview,
        temp,
        session,
        asset_id,
        source,
        export_dir,
    }
}

impl ExportFixture {
    fn export(&self, ids: Vec<i64>) -> Result<Value, Value> {
        invoke(
            &self.webview,
            "asset_export",
            json!({
                "ids": ids,
                "targetDir": self.export_dir.to_string_lossy(),
            }),
        )
    }
}

#[cfg(unix)]
#[test]
fn export_does_not_write_through_a_dangling_destination_symlink() {
    // QA-005: exercise the real IPC export command; every path stays in this fixture.
    let fixture = export_fixture();
    let outside_dir = fixture.temp.path().join("outside-export");
    std::fs::create_dir(&outside_dir).unwrap();
    let escaped = outside_dir.join("unexpected.png");
    let link = fixture.export_dir.join("photo.png");
    std::os::unix::fs::symlink(&escaped, &link).unwrap();

    let result = fixture.export(vec![fixture.asset_id]);
    assert!(
        result.is_ok(),
        "export should choose a free filename: {result:?}"
    );
    assert!(
        !escaped.exists(),
        "asset_export followed a dangling symlink and wrote outside the selected directory"
    );
    let paths = result.unwrap();
    assert_eq!(paths.as_array().unwrap().len(), 1);
    let output = std::path::Path::new(paths[0].as_str().unwrap());
    assert_eq!(output, fixture.export_dir.join("photo (1).png"));
    assert!(!std::fs::symlink_metadata(output)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(output).unwrap(),
        std::fs::read(&fixture.source).unwrap()
    );
    assert_eq!(std::fs::read_link(link).unwrap(), escaped);
}

#[test]
fn export_preserves_existing_files_and_directories_and_returns_complete_copies() {
    let fixture = export_fixture();
    let occupied = fixture.export_dir.join("photo.png");
    std::fs::write(&occupied, b"keep existing file").unwrap();
    let occupied_dir = fixture.export_dir.join("photo (1).png");
    std::fs::create_dir(&occupied_dir).unwrap();
    let paths = fixture
        .export(vec![fixture.asset_id, fixture.asset_id])
        .unwrap();
    for (index, path) in paths.as_array().unwrap().iter().enumerate() {
        let output = std::path::Path::new(path.as_str().unwrap());
        assert_eq!(
            output,
            fixture
                .export_dir
                .join(format!("photo ({}).png", index + 2))
        );
        assert_eq!(
            std::fs::read(output).unwrap(),
            std::fs::read(&fixture.source).unwrap()
        );
    }
    assert_eq!(std::fs::read(occupied).unwrap(), b"keep existing file");
    assert!(occupied_dir.is_dir());
}

#[cfg(unix)]
#[test]
fn export_preserves_live_symlinks_and_their_targets() {
    let fixture = export_fixture();
    let outside = fixture.temp.path().join("outside.png");
    std::fs::write(&outside, b"keep symlink target").unwrap();
    let link = fixture.export_dir.join("photo.png");
    std::os::unix::fs::symlink(&outside, &link).unwrap();
    let paths = fixture.export(vec![fixture.asset_id]).unwrap();
    assert_eq!(
        std::path::Path::new(paths[0].as_str().unwrap()),
        fixture.export_dir.join("photo (1).png")
    );
    assert_eq!(std::fs::read_link(link).unwrap(), outside);
    assert_eq!(std::fs::read(outside).unwrap(), b"keep symlink target");
}

#[test]
fn export_rejects_asset_names_that_escape_the_selected_directory() {
    let fixture = export_fixture();
    let absolute = fixture
        .temp
        .path()
        .join("escaped.png")
        .to_string_lossy()
        .into_owned();
    for name in [
        "../escaped.png",
        "nested/photo.png",
        "nested\\photo.png",
        "",
        ".",
        "..",
        absolute.as_str(),
    ] {
        fixture
            .session
            .db
            .lock()
            .unwrap()
            .execute(
                "UPDATE assets SET file_name = ?1 WHERE id = ?2",
                rusqlite::params![name, fixture.asset_id],
            )
            .unwrap();
        let error = fixture.export(vec![fixture.asset_id]).unwrap_err();
        assert_eq!(error["code"], "invalid_argument", "filename: {name}");
        assert_eq!(std::fs::read_dir(&fixture.export_dir).unwrap().count(), 0);
        assert!(!fixture.temp.path().join("escaped.png").exists());
    }
}

#[test]
fn concurrent_exports_create_distinct_complete_files() {
    let fixture = export_fixture();
    let barrier = Arc::new(std::sync::Barrier::new(4));
    let workers: Vec<_> = (0..4)
        .map(|_| {
            let webview = fixture.webview.clone();
            let barrier = barrier.clone();
            let asset_id = fixture.asset_id;
            let export_dir = fixture.export_dir.clone();
            std::thread::spawn(move || {
                barrier.wait();
                invoke(
                    &webview,
                    "asset_export",
                    json!({ "ids": [asset_id], "targetDir": export_dir }),
                )
                .unwrap()
            })
        })
        .collect();
    let mut paths = std::collections::HashSet::new();
    for worker in workers {
        let result = worker.join().unwrap();
        let output = result[0].as_str().unwrap();
        assert!(paths.insert(output.to_owned()), "exports reused {output}");
        assert_eq!(
            std::fs::read(output).unwrap(),
            std::fs::read(&fixture.source).unwrap()
        );
    }
    assert_eq!(std::fs::read_dir(&fixture.export_dir).unwrap().count(), 4);
}

#[test]
fn export_supports_managed_assets_and_unicode_names_without_extensions() {
    let fixture = export_fixture_with_mode(import::ImportMode::Copy);
    std::fs::remove_file(&fixture.source).unwrap();
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE assets SET file_name = '照片' WHERE id = ?1",
            [fixture.asset_id],
        )
        .unwrap();
    let paths = fixture
        .export(vec![fixture.asset_id, fixture.asset_id])
        .unwrap();
    assert_eq!(
        std::path::Path::new(paths[0].as_str().unwrap()),
        fixture.export_dir.join("照片")
    );
    assert_eq!(
        std::path::Path::new(paths[1].as_str().unwrap()),
        fixture.export_dir.join("照片 (1)")
    );
    assert_eq!(
        std::fs::read(paths[0].as_str().unwrap()).unwrap(),
        std::fs::read(paths[1].as_str().unwrap()).unwrap()
    );
    let bytes = std::fs::read(paths[0].as_str().unwrap()).unwrap();
    assert_eq!(image::load_from_memory(&bytes).unwrap().width(), 8);
}

#[test]
fn export_of_a_missing_reference_creates_no_output() {
    let fixture = export_fixture();
    std::fs::remove_file(&fixture.source).unwrap();
    let error = fixture.export(vec![fixture.asset_id]).unwrap_err();
    assert_eq!(error["code"], "source_file_missing");
    assert_eq!(std::fs::read_dir(&fixture.export_dir).unwrap().count(), 0);
}

#[test]
fn reading_current_library_does_not_block_ipc_dispatch_on_config_lock() {
    dispatch_while_locked("library_current", json!({}), true);
}

#[test]
fn retry_and_recovery_queries_dispatch_without_waiting_for_database() {
    for (command, body) in [
        ("import_retry", json!({ "ids": null })),
        ("import_recovery_list", json!({})),
        ("asset_preview", json!({ "id": 999 })),
        ("folder_create", json!({ "name": "Background" })),
        ("asset_add_tags", json!({ "id": 999, "names": ["Test"] })),
        ("tag_rename", json!({ "id": 1, "newName": "Background" })),
    ] {
        dispatch_while_locked(command, body, false);
    }
}

#[test]
fn retry_rejects_active_processing_without_clearing_its_cancel_flag() {
    use std::sync::atomic::Ordering;
    let app = test_app();
    let temp = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&temp.path().join("library"), "Test").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    let state = app.state::<AppState>();
    *state.session.write().unwrap() = Some(session);
    state.import_ctl.running.store(true, Ordering::SeqCst);
    state.import_ctl.cancel.store(true, Ordering::SeqCst);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let error = invoke(&webview, "import_retry", json!({ "ids": [999] })).unwrap_err();
    assert_error_code(&error, "operation_busy");
    assert!(state.import_ctl.running.load(Ordering::SeqCst));
    assert!(state.import_ctl.cancel.load(Ordering::SeqCst));
}

#[test]
fn failed_retry_releases_the_processing_slot_and_preserves_result_contract() {
    use std::sync::atomic::Ordering;
    let app = test_app();
    let temp = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&temp.path().join("library"), "Test").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    *app.state::<AppState>().session.write().unwrap() = Some(session);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let result = invoke(&webview, "import_retry", json!({ "ids": [999] })).unwrap();
    assert_eq!(result, json!({ "retried": 0, "failed": 1 }));
    assert!(!app
        .state::<AppState>()
        .import_ctl
        .running
        .load(Ordering::SeqCst));
}

fn invoke(
    webview: &tauri::WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
) -> Result<Value, Value> {
    invoke_with_headers(webview, command, body, Default::default())
}

fn invoke_with_headers(
    webview: &tauri::WebviewWindow<MockRuntime>,
    command: &str,
    body: Value,
    headers: tauri::http::HeaderMap,
) -> Result<Value, Value> {
    get_ipc_response(
        webview,
        InvokeRequest {
            cmd: command.into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: if cfg!(any(windows, target_os = "android")) {
                "http://tauri.localhost"
            } else {
                "tauri://localhost"
            }
            .parse()
            .unwrap(),
            body: InvokeBody::Json(body),
            headers,
            invoke_key: INVOKE_KEY.to_string(),
        },
    )
    .map(|body| body.deserialize::<Value>().unwrap())
}

fn assert_error_code(error: &Value, expected: &str) {
    assert_eq!(
        error.get("code").and_then(Value::as_str),
        Some(expected),
        "{error}"
    );
    assert!(
        error
            .get("message")
            .and_then(Value::as_str)
            .is_some_and(|message| !message.is_empty()),
        "structured command errors must include a diagnostic message: {error}"
    );
}

#[test]
fn read_commands_return_default_state_through_ipc() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let current = invoke(&webview, "library_current", json!({})).unwrap();
    assert_eq!(current, Value::Null);

    let settings = invoke(&webview, "settings_get", json!({})).unwrap();
    assert_eq!(settings["language"], "en");
    assert_eq!(settings["theme"], "auto");
    assert_eq!(settings["trashRetentionDays"], 30);
    assert_eq!(settings["importMode"], "reference");
}

#[test]
fn invalid_folder_and_tag_names_keep_stable_error_codes() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    for name in ["", ".hidden", "nested/name", "drive:name"] {
        let error = invoke(&webview, "folder_create", json!({ "name": name })).unwrap_err();
        assert_error_code(&error, "invalid_argument");
    }

    let error = invoke(
        &webview,
        "tag_rename",
        json!({ "id": 1, "newName": "one,two" }),
    )
    .unwrap_err();
    assert_error_code(&error, "tag_name_invalid");
}

#[test]
fn session_commands_reject_requests_when_no_library_is_open() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let cases = [
        ("folder_create", json!({ "name": "Valid" })),
        ("folder_rename", json!({ "id": 1, "newName": "Renamed" })),
        ("asset_trash", json!({ "ids": [1, 2] })),
        ("asset_playback_source", json!({ "id": 1 })),
        ("asset_playback_codec", json!({ "id": 1 })),
    ];
    for (command, body) in cases {
        let error = invoke(&webview, command, body).unwrap_err();
        assert_error_code(&error, "library_not_open");
    }
}

#[test]
fn mutating_an_unknown_library_returns_not_found() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let error = invoke(
        &webview,
        "library_rename",
        json!({ "id": "missing-library", "name": "Missing", "color": null }),
    )
    .unwrap_err();
    assert_error_code(&error, "library_not_found");
}

#[test]
fn malformed_ipc_arguments_are_rejected_before_command_execution() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();

    let error = invoke(
        &webview,
        "folder_rename",
        json!({ "id": "not-an-integer", "newName": "Valid" }),
    )
    .unwrap_err();
    let message = error.as_str().unwrap_or_default();
    assert!(
        message.contains("invalid type") || message.contains("integer"),
        "{error}"
    );
}

#[test]
fn creates_standalone_tags_atomically_through_ipc() {
    let app = test_app();
    let dir = tempfile::tempdir().unwrap();
    let meta = clip_studio_lib::library::init_library(&dir.path().join("library"), "Tags").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let count = invoke(
        &webview,
        "tag_create_many",
        json!({"names":[" Test ","Test","旅行"]}),
    )
    .unwrap();
    assert_eq!(count, json!(2));
    let count = invoke(&webview, "tag_create_many", json!({"names":["Test"]})).unwrap();
    assert_eq!(count, json!(0));
    assert!(invoke(
        &webview,
        "tag_create_many",
        json!({"names":["valid","bad,name"]})
    )
    .is_err());
    let tags = clip_studio_lib::db::tags::list(&session.read_db()).unwrap();
    assert_eq!(tags.len(), 2);
    assert!(tags.iter().all(|t| t.asset_count == 0));
    let result = invoke(
        &webview,
        "target_options",
        json!({"kind":"tags","query":"旅","after":0,"exclude":[]}),
    )
    .unwrap();
    assert_eq!(result["items"].as_array().unwrap().len(), 1);
    let source = tags[0].id;
    let target = tags[1].id;
    assert!(invoke(
        &webview,
        "tag_batch",
        json!({"ids":[source],"targetId":source})
    )
    .is_err());
    invoke(
        &webview,
        "tag_batch",
        json!({"ids":[source],"targetId":target}),
    )
    .unwrap();
    invoke(
        &webview,
        "tag_batch",
        json!({"ids":[source,target],"targetId":null}),
    )
    .unwrap();
    assert!(clip_studio_lib::db::tags::list(&session.read_db())
        .unwrap()
        .is_empty());
    let names = (0..120).map(|i| format!("Page {i}")).collect::<Vec<_>>();
    invoke(&webview, "tag_create_many", json!({"names":names})).unwrap();
    let first = invoke(
        &webview,
        "target_options",
        json!({"kind":"tags","query":"Page","after":0,"exclude":[]}),
    )
    .unwrap();
    assert_eq!(first["items"].as_array().unwrap().len(), 50);
    assert_eq!(first["hasMore"], json!(true));
    let cursor = first["items"][49]["id"].as_i64().unwrap();
    let next = invoke(
        &webview,
        "target_options",
        json!({"kind":"tags","query":"Page","after":cursor,"exclude":[]}),
    )
    .unwrap();
    assert_eq!(next["items"].as_array().unwrap().len(), 50);
    assert!(next["items"][0]["id"].as_i64().unwrap() > cursor);
}

#[test]
fn folder_batch_validates_targets_and_preserves_sources_on_move() {
    let app = test_app();
    let dir = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&dir.path().join("library"), "Folders").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    let a = clip_studio_lib::services::folders::create_folder(&session, "A").unwrap();
    let b = clip_studio_lib::services::folders::create_folder(&session, "B").unwrap();
    *app.state::<AppState>().session.write().unwrap() = Some(session.clone());
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(
        &webview,
        "folder_batch",
        json!({"ids":[a.id],"targetId":a.id})
    )
    .is_err());
    invoke(
        &webview,
        "folder_batch",
        json!({"ids":[a.id],"targetId":b.id}),
    )
    .unwrap();
    assert_eq!(
        clip_studio_lib::db::folders::list(&session.read_db())
            .unwrap()
            .len(),
        2
    );
    invoke(
        &webview,
        "folder_batch",
        json!({"ids":[a.id,b.id],"targetId":null}),
    )
    .unwrap();
    assert!(clip_studio_lib::db::folders::list(&session.read_db())
        .unwrap()
        .is_empty());
}

#[path = "command_boundary/management.rs"]
mod management_regressions;

#[path = "command_boundary/settings.rs"]
mod settings_regressions;

#[path = "command_boundary/cover.rs"]
mod cover_regressions;

#[path = "command_boundary/indexing.rs"]
mod indexing;

#[test]
fn folder_pagination_and_sidebar_commands_guard_library_identity() {
    let app = test_app();
    let dir = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&dir.path().join("library"), "Folders").unwrap();
    let library_id = meta.id.clone();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    let folder = clip_studio_lib::services::folders::create_folder(&session, "旅行 %_").unwrap();
    *app.state::<AppState>().session.write().unwrap() = Some(session);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let page = invoke(&webview, "folder_page", json!({"query":"%_","page":1})).unwrap();
    assert_eq!(page["total"], 1);
    assert_eq!(page["hasMore"], false);
    assert_eq!(
        invoke(&webview, "folder_get", json!({"id":folder.id})).unwrap()["name"],
        "旅行 %_"
    );
    assert!(invoke(
        &webview,
        "folder_sidebar_set",
        json!({"libraryId":"other","ids":[]})
    )
    .is_err());
    assert!(invoke(
        &webview,
        "folder_pin",
        json!({"libraryId":"other","id":folder.id,"pinned":false})
    )
    .is_err());
    assert_eq!(
        invoke(&webview, "folder_sidebar", json!({})).unwrap()["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    invoke(
        &webview,
        "folder_sidebar_set",
        json!({"libraryId":library_id,"ids":[]}),
    )
    .unwrap();
    let result = invoke(
        &webview,
        "folder_pin",
        json!({"libraryId":library_id,"id":folder.id,"pinned":true}),
    )
    .unwrap();
    assert_eq!(result["items"][0]["id"], folder.id);
    assert_eq!(result["displayLimit"], 10);
    assert!(invoke(
        &webview,
        "folder_sidebar_set",
        json!({"libraryId":"other","ids":[folder.id],"displayLimit":5})
    )
    .is_err());
    for limit in [4, 31] {
        assert!(invoke(
            &webview,
            "folder_sidebar_set",
            json!({"libraryId":library_id,"ids":[],"displayLimit":limit})
        )
        .is_err());
        assert_eq!(
            invoke(&webview, "folder_sidebar", json!({})).unwrap()["items"][0]["id"],
            folder.id
        );
    }
    let result = invoke(
        &webview,
        "folder_sidebar_set",
        json!({"libraryId":library_id,"ids":[folder.id],"displayLimit":30}),
    )
    .unwrap();
    assert_eq!(result["displayLimit"], 30);
    assert_eq!(
        invoke(&webview, "folder_sidebar", json!({})).unwrap()["displayLimit"],
        30
    );
}

#[test]
fn folder_sorting_defaults_to_newest_and_rejects_unknown_orders() {
    let app = test_app();
    let dir = tempfile::tempdir().unwrap();
    let meta =
        clip_studio_lib::library::init_library(&dir.path().join("library"), "Sorting").unwrap();
    let session = Arc::new(clip_studio_lib::library::open_session(meta).unwrap());
    {
        let conn = session.db.lock().unwrap();
        for (name, time) in [("Alpha", 100), ("Zulu", 200), ("Middle", 200)] {
            clip_studio_lib::db::folders::insert(&conn, name, time).unwrap();
        }
    }
    *app.state::<AppState>().session.write().unwrap() = Some(session);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let default = invoke(&webview, "folder_page", json!({"query":"","page":1})).unwrap();
    assert_eq!(default["items"][0]["name"], "Middle");
    for (sort, first) in [
        ("created_desc", "Middle"),
        ("created_asc", "Alpha"),
        ("name_asc", "Alpha"),
        ("name_desc", "Zulu"),
    ] {
        let result = invoke(
            &webview,
            "folder_page",
            json!({"query":"","page":1,"sort":sort}),
        )
        .unwrap();
        assert_eq!(result["items"][0]["name"], first);
    }
    assert!(invoke(
        &webview,
        "folder_page",
        json!({"query":"","page":1,"sort":"bad"})
    )
    .is_err());
}

#[test]
fn moving_assets_through_ipc_preserves_files_and_target_errors() {
    let fixture = export_fixture();
    let target = {
        let conn = fixture.session.db.lock().unwrap();
        clip_studio_lib::db::folders::insert(&conn, "Move target", 1).unwrap()
    };
    invoke(
        &fixture.webview,
        "asset_move_to_folder",
        json!({"ids": [fixture.asset_id], "folderId": target}),
    )
    .unwrap();
    let current_folder = || {
        let conn = fixture.session.db.lock().unwrap();
        clip_studio_lib::db::assets::get_asset(&conn, &fixture.session.root, fixture.asset_id)
            .unwrap()
            .unwrap()
            .folder_id
    };
    assert_eq!(current_folder(), Some(target));
    let error = invoke(
        &fixture.webview,
        "asset_move_to_folder",
        json!({"ids": [fixture.asset_id], "folderId": 999}),
    )
    .unwrap_err();
    assert_error_code(&error, "folder_not_found");
    assert_eq!(current_folder(), Some(target));
    invoke(
        &fixture.webview,
        "asset_move_to_folder",
        json!({"ids": [fixture.asset_id], "folderId": null}),
    )
    .unwrap();
    assert_eq!(current_folder(), None);
    assert!(fixture.source.is_file());
}

#[test]
fn empty_semantic_queries_keep_the_ipc_contract_without_a_library_or_models() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let result = invoke(
        &webview,
        "search_text",
        json!({"query": "   ", "filter": null, "page": 7, "pageSize": 100}),
    )
    .unwrap();
    assert_eq!(
        result,
        json!({"items": [], "total": 0, "page": 1, "pageSize": 0, "hasMore": false})
    );
    assert!(app.state::<AppState>().session.read().unwrap().is_none());
    assert!(!app.state::<AppState>().clip.visual_ready());
}
