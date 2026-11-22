//! Real Streamable HTTP requests against the restored MCP server and shared services.
use clip_studio_lib::{
    clip, config::GlobalConfig, db, import, library, mcp, services, vector, AppState,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex, RwLock},
    time::{Duration, Instant},
};
use tauri::{
    test::{mock_builder, mock_context, noop_assets, MockRuntime},
    Manager,
};

struct Harness {
    temp: tempfile::TempDir,
    handle: tauri::AppHandle<MockRuntime>,
    first: Arc<library::LibrarySession>,
    second: String,
    uid: String,
    url: String,
    token: String,
    client: reqwest::Client,
}
impl Drop for Harness {
    fn drop(&mut self) {
        self.handle.state::<AppState>().mcp.stop();
    }
}
impl Harness {
    async fn new(write: bool) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let first = library::init_library(&temp.path().join("first"), "First").unwrap();
        let second = library::init_library(&temp.path().join("second"), "Second").unwrap();
        let session = Arc::new(library::open_session(first.clone()).unwrap());
        let source = temp.path().join("photo.png");
        image::RgbImage::from_pixel(24, 24, image::Rgb([255, 0, 0]))
            .save(&source)
            .unwrap();
        let result = import::run_import_with_policy(
            &session,
            &[source],
            None,
            &AtomicBool::new(false),
            import::ImportMode::Reference,
            db::indexing::IndexPolicy {
                visual: false,
                subtitles: false,
            },
            import::ImportCallbacks {
                progress: &|_, _, _| {},
                on_asset: &|_| {},
            },
        );
        assert_eq!(result.done, 1);
        let uid = session
            .read_db()
            .query_row("SELECT uid FROM assets", [], |r| r.get(0))
            .unwrap();
        let mut config = GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
        config.libraries = vec![first.clone(), second.clone()];
        config.active_library_id = Some(first.id);
        config.settings.mcp_enabled = true;
        config.settings.mcp_write_enabled = write;
        config.settings.auto_visual_index = false;
        config.save().unwrap();
        let state = AppState {
            mcp: Default::default(),
            config: RwLock::new(config),
            session: RwLock::new(Some(session.clone())),
            session_cache: Mutex::new(HashMap::new()),
            session_switch_lock: Mutex::new(()),
            import_ctl: Default::default(),
            clip: clip::ClipHandle::default(),
            operation_running: AtomicBool::new(false),
            source_check_running: Arc::new(AtomicBool::new(false)),
            source_check_cancel: Arc::new(AtomicBool::new(false)),
        };
        let app = mock_builder()
            .manage(state)
            .build(mock_context(noop_assets()))
            .unwrap();
        let handle = app.handle().clone();
        drop(app);
        let port = mcp::apply_settings(&handle).await.unwrap();
        let token = mcp::reveal_token(&handle.state::<AppState>()).unwrap();
        Self {
            temp,
            handle,
            first: session,
            second: second.id,
            uid,
            url: format!("http://127.0.0.1:{port}/mcp"),
            token,
            client: reqwest::Client::builder()
                .no_proxy()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }
    async fn raw_rpc(&self, method: &str, params: Value) -> Value {
        let response = self
            .client
            .post(&self.url)
            .bearer_auth(&self.token)
            .header("Accept", "application/json, text/event-stream")
            .json(&json!({"jsonrpc":"2.0","id":1,"method":method,"params":params}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 200);
        let value: Value = response.json().await.unwrap();
        value
    }
    async fn rpc(&self, method: &str, params: Value) -> Value {
        let value = self.raw_rpc(method, params).await;
        assert!(value.get("error").is_none(), "{value}");
        value["result"].clone()
    }
    async fn call(&self, name: &str, args: Value) -> Value {
        self.rpc("tools/call", json!({"name":name,"arguments":args}))
            .await
    }
}
#[tokio::test]
async fn authenticated_protocol_lists_only_supported_tools_and_preserves_session_selection() {
    let h = Harness::new(false).await;
    let response = h.client.post(&h.url).json(&json!({})).send().await.unwrap();
    assert_eq!(response.status(), 401);
    let bad_host = h
        .client
        .post(&h.url)
        .bearer_auth(&h.token)
        .header("Host", "evil.example")
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(bad_host.status(), 403);
    let init=h.rpc("initialize",json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}})).await;
    assert_eq!(init["serverInfo"]["name"], "clipseek-open");
    let list = h.rpc("tools/list", json!({})).await;
    let mut names = list["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = vec![
        "list_libraries",
        "get_library_context",
        "search_assets",
        "get_asset",
        "get_asset_metadata",
        "create_folder",
        "update_asset_tags",
        "move_assets_to_folder",
        "import_assets",
        "get_operation",
    ];
    expected.sort();
    assert_eq!(names, expected);
    let assets = h
        .call("search_assets", json!({"filenameContains":"photo"}))
        .await;
    assert_eq!(assets["structuredContent"]["total"], 1);
    let empty = h.call("search_assets", json!({"libraryId":h.second})).await;
    assert_eq!(empty["structuredContent"]["total"], 0);
    assert!(Arc::ptr_eq(
        h.handle
            .state::<AppState>()
            .session
            .read()
            .unwrap()
            .as_ref()
            .unwrap(),
        &h.first
    ));
    let detail = h.call("get_asset", json!({"assetUid":h.uid})).await;
    let uri = detail["structuredContent"]["asset"]["previewUri"]
        .as_str()
        .unwrap();
    let preview = h.rpc("resources/read", json!({"uri":uri})).await;
    assert_eq!(preview["contents"][0]["mimeType"], "image/webp");
    assert!(preview["contents"][0]["blob"].as_str().unwrap().len() > 20);
    let denied = h.call("create_folder", json!({"name":"Denied"})).await;
    assert_eq!(denied["isError"], true);
    assert_eq!(denied["structuredContent"]["code"], "write_disabled");
    let traversal = h
        .raw_rpc(
            "resources/read",
            json!({"uri":"clipstudio-open://libraries/../assets/../preview"}),
        )
        .await;
    assert!(traversal.get("error").is_some());
}
#[tokio::test]
async fn writes_share_the_durable_reference_queue_and_tokens_are_revocable() {
    let mut h = Harness::new(true).await;
    let folder = h
        .call(
            "create_folder",
            json!({"libraryId":h.second,"name":"New folder"}),
        )
        .await;
    assert_eq!(folder["structuredContent"]["folder"]["name"], "New folder");
    let tags = h
        .call(
            "update_asset_tags",
            json!({"assetUid":h.uid,"add":["Red"],"remove":[]}),
        )
        .await;
    assert_eq!(tags["structuredContent"]["tags"], json!(["Red"]));
    let source = h.temp.path().join("import.png");
    image::RgbImage::from_pixel(16, 16, image::Rgb([0, 0, 255]))
        .save(&source)
        .unwrap();
    let bytes = std::fs::read(&source).unwrap();
    let accepted = h
        .call(
            "import_assets",
            json!({"libraryId":h.second,"paths":[source]}),
        )
        .await;
    assert_ne!(accepted["isError"], true, "{accepted}");
    let operation = accepted["structuredContent"]["operationId"]
        .as_str()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let state = h
            .call(
                "get_operation",
                json!({"libraryId":h.second,"operationId":operation}),
            )
            .await;
        let summary = &state["structuredContent"]["summary"];
        if summary["completed"] == 1 {
            assert_eq!(summary["failed"], 0);
            break;
        }
        assert!(Instant::now() < deadline, "{state}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let s = services::session(&h.handle.state::<AppState>(), Some(&h.second)).unwrap();
    let source_path: String = s
        .read_db()
        .query_row("SELECT source_path FROM assets", [], |r| r.get(0))
        .unwrap();
    assert_eq!(PathBuf::from(source_path), source);
    assert_eq!(std::fs::read(source).unwrap(), bytes);
    let old = h.token.clone();
    h.token = mcp::rotate_token(&h.handle.state::<AppState>()).unwrap();
    assert_ne!(old, h.token);
    let rejected = h
        .client
        .post(&h.url)
        .bearer_auth(old)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert_eq!(rejected.status(), 401);
    assert_eq!(
        h.rpc("tools/list", json!({})).await["tools"]
            .as_array()
            .unwrap()
            .len(),
        10
    );
    assert!(h.temp.path().join("mcp-token").exists());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            std::fs::metadata(h.temp.path().join("mcp-token"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
    h.handle
        .state::<AppState>()
        .config
        .write()
        .unwrap()
        .settings
        .mcp_enabled = false;
    mcp::apply_settings(&h.handle).await.unwrap();
    let settings = h
        .handle
        .state::<AppState>()
        .config
        .read()
        .unwrap()
        .settings
        .clone();
    assert!(!h.handle.state::<AppState>().mcp.status(&settings).running);
}
#[tokio::test]
#[ignore = "requires CLIPSTUDIO_MODELS; run explicitly"]
async fn semantic_search_uses_the_shared_embeddinggemma_index() {
    let h = Harness::new(false).await;
    let id = h
        .first
        .read_db()
        .query_row("SELECT id FROM assets", [], |r| r.get(0))
        .unwrap();
    // This fixture imports with indexing disabled; explicitly request the visual
    // branch before preparing its input, as the desktop queue does.
    db::indexing::set_policy(
        &h.first.db.lock().unwrap(),
        id,
        db::indexing::IndexPolicy {
            visual: true,
            subtitles: false,
        },
    )
    .unwrap();
    import::build_visual_index(&h.first, id).unwrap();
    let encoder = h.handle.state::<AppState>().clip.get_or_init().unwrap();
    assert_eq!(
        vector::embed_pending(&h.first, &encoder, &|_| {}).unwrap(),
        1
    );
    let result = h
        .call("search_assets", json!({"semanticQuery":"a red image"}))
        .await;
    assert_ne!(result["isError"], true, "{result}");
    assert_eq!(result["structuredContent"]["total"], 1);
    assert_eq!(result["structuredContent"]["items"][0]["assetUid"], h.uid);
}
