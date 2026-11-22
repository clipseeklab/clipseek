//! Precision changes preserve the original session guards and invalidate all registered libraries.
use crate::{clip, config::GlobalConfig, db, import, library, services, AppState};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, RwLock,
    },
};
use tauri::{
    ipc::{CallbackFn, InvokeBody},
    test::{get_ipc_response, mock_builder, mock_context, noop_assets, MockRuntime, INVOKE_KEY},
    webview::InvokeRequest,
    Manager,
};
fn invoke(
    window: &tauri::WebviewWindow<MockRuntime>,
    precision: &str,
    root: &str,
) -> Result<Value, Value> {
    get_ipc_response(
        window,
        InvokeRequest {
            cmd: "model_settings".into(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "tauri://localhost".parse().unwrap(),
            body: InvokeBody::Json(json!({"precision":precision,"modelDirectory":root})),
            headers: Default::default(),
            invoke_key: INVOKE_KEY.into(),
        },
    )
    .map(|r| r.deserialize::<Value>().unwrap())
}
#[test]
fn switching_precision_rejects_active_work_and_invalidates_every_library() {
    let temp = tempfile::tempdir().unwrap();
    let mut cfg = GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
    let mut sessions = Vec::new();
    for name in ["First", "Second"] {
        let meta = library::init_library(&temp.path().join(name), name).unwrap();
        cfg.libraries.push(meta.clone());
        let s = Arc::new(library::open_session(meta).unwrap());
        {
            let c = s.db.lock().unwrap();
            let id = db::assets::insert_asset(
                &c,
                db::assets::NewAsset {
                    file_name: "photo.png",
                    rel_path: "photo.png",
                    file_hash: name,
                    mime_type: "image/png",
                    kind: "image",
                    file_size: 1,
                    imported_at: 1,
                    ..Default::default()
                },
            )
            .unwrap();
            c.execute(
                "UPDATE assets SET import_stage='done',visual_indexed=1 WHERE id=?",
                [id],
            )
            .unwrap();
            let frame = db::assets::insert_frame(&c, id, 0, 0, None).unwrap();
            c.execute("UPDATE frames SET embedded=1 WHERE id=?", [frame])
                .unwrap();
        }
        sessions.push(s);
    }
    cfg.active_library_id = Some(sessions[0].meta.id.clone());
    cfg.save().unwrap();
    let cache = sessions
        .iter()
        .map(|s| (s.meta.id.clone(), Arc::downgrade(s)))
        .collect::<HashMap<_, _>>();
    let state = AppState {
        mcp: Default::default(),
        config: RwLock::new(cfg),
        session: RwLock::new(Some(sessions[0].clone())),
        session_cache: Mutex::new(cache),
        session_switch_lock: Mutex::new(()),
        import_ctl: import::ImportControl::default(),
        clip: clip::ClipHandle::default(),
        operation_running: AtomicBool::new(false),
        source_check_running: Arc::new(AtomicBool::new(false)),
        source_check_cancel: Arc::new(AtomicBool::new(false)),
    };
    let app = mock_builder()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            crate::commands::model::model_settings
        ])
        .build(mock_context(noop_assets()))
        .unwrap();
    let window = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let root = temp.path().join("models").to_string_lossy().into_owned();
    assert_eq!(
        invoke(&window, "q4", "relative").unwrap_err()["code"],
        "invalid_argument"
    );
    app.state::<AppState>()
        .import_ctl
        .running
        .store(true, Ordering::Release);
    assert_eq!(
        invoke(&window, "q4", &root).unwrap_err()["code"],
        "operation_busy"
    );
    app.state::<AppState>()
        .import_ctl
        .running
        .store(false, Ordering::Release);
    let work = sessions[1].begin_asset_work(1).unwrap();
    assert_eq!(
        invoke(&window, "q4", &root).unwrap_err()["code"],
        "operation_busy"
    );
    drop(work);
    let status = invoke(&window, "q4", &root).unwrap();
    assert_eq!(status["precision"], "q4");
    assert_eq!(status["ready"], false);
    for s in &sessions {
        assert_eq!(
            s.read_db()
                .query_row("SELECT SUM(visual_indexed) FROM assets", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            s.read_db()
                .query_row("SELECT SUM(embedded) FROM frames", [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(db::indexing::tasks(&s.read_db()).unwrap().len(), 1);
        assert_eq!(
            services::search::run_search_session(s, vec![1.; 768], None, None, None)
                .unwrap()
                .total,
            0
        );
    }
    assert_eq!(
        app.state::<AppState>()
            .config
            .read()
            .unwrap()
            .settings
            .embedding_precision,
        crate::embedding::domain::Precision::Q4
    );
    for s in &sessions {
        assert_eq!(
            db::indexing::tasks(&s.read_db()).unwrap()[0].status,
            "queued"
        );
        assert!(!s.index_worker_running.load(Ordering::Acquire));
    }
    for s in &sessions {
        drop(s.vector.lock().unwrap().take());
    }
}
