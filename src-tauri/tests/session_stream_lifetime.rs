//! Live file responses retain their source session until consumed or discarded.

use std::collections::HashMap;
use std::sync::{atomic::AtomicBool, Arc, Mutex, RwLock, Weak};

use axum::{
    body::{to_bytes, Body},
    http::{Request, Response, StatusCode},
    routing::get,
    Router,
};
use clip_studio_lib::config::{GlobalConfig, LibraryMeta};
use clip_studio_lib::library::{self, LibrarySession};
use clip_studio_lib::playback_media::{download, DownloadRegistry};
use clip_studio_lib::{clip, import, services, AppState};
use tower::ServiceExt;

struct Fixture {
    directory: tempfile::TempDir,
    state: AppState,
    first: LibraryMeta,
    second: LibraryMeta,
    session: Weak<LibrarySession>,
    registry: Arc<DownloadRegistry>,
    url: String,
}

impl Fixture {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let first = library::init_library(&directory.path().join("first"), "First").unwrap();
        let second = library::init_library(&directory.path().join("second"), "Second").unwrap();
        let mut config = GlobalConfig::load_from(directory.path().join("config.json")).unwrap();
        config.libraries = vec![first.clone(), second.clone()];
        config.active_library_id = Some(first.id.clone());
        config.save().unwrap();
        let state = AppState {
            mcp: Default::default(),
            config: RwLock::new(config),
            session: RwLock::new(None),
            session_cache: Mutex::new(HashMap::new()),
            session_switch_lock: Mutex::new(()),
            import_ctl: import::ImportControl::default(),
            clip: clip::ClipHandle::default(),
            operation_running: AtomicBool::new(false),
            source_check_running: Arc::new(AtomicBool::new(false)),
            source_check_cancel: Arc::new(AtomicBool::new(false)),
        };
        // Use the same shared-session entry point as startup and library switching.
        let session = services::sessions::resolve(&state, Some(&first.id)).unwrap();
        let weak = Arc::downgrade(&session);
        let source = session.root.join("images/download.bin");
        std::fs::write(&source, b"0123456789").unwrap();
        *state.session.write().unwrap() = Some(session);
        let registry = Arc::new(DownloadRegistry::for_local_playback());
        let grant = registry
            .issue_inline(
                source,
                "download.bin".into(),
                "application/octet-stream".into(),
                "http://localhost",
                60,
            )
            .unwrap();
        Self {
            directory,
            state,
            first,
            second,
            session: weak,
            registry,
            url: grant.url,
        }
    }

    async fn response(&self, method: &str, range: &str) -> Response<Body> {
        Router::new()
            .route("/files/{download_id}/{file_name}", get(download))
            .with_state(self.registry.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(&self.url)
                    .header("range", range)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn switch_away(&self) {
        let next = services::sessions::resolve(&self.state, Some(&self.second.id)).unwrap();
        *self.state.session.write().unwrap() = Some(next);
        self.state.config.write().unwrap().active_library_id = Some(self.second.id.clone());
    }

    fn resolve_source(&self) -> Arc<LibrarySession> {
        services::sessions::resolve(&self.state, Some(&self.first.id))
            .expect("a healthy source library must remain accessible while its download is open")
    }
}

#[tokio::test]
async fn resolving_a_switched_away_library_keeps_its_unconsumed_download_usable() {
    let fixture = Fixture::new();
    let response = fixture.response("GET", "bytes=0-").await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()["content-length"], "10");
    fixture.switch_away();

    let reopened = fixture.resolve_source();
    assert_eq!(reopened.meta.id, fixture.first.id);
    assert!(Arc::ptr_eq(&reopened, &fixture.session.upgrade().unwrap()));
    assert_eq!(
        services::sessions::current(&fixture.state).unwrap().meta.id,
        fixture.second.id
    );
    assert_eq!(
        to_bytes(response.into_body(), 10).await.unwrap().as_ref(),
        b"0123456789"
    );
}

#[tokio::test]
async fn consuming_a_download_releases_its_library_for_session_reopen() {
    let fixture = Fixture::new();
    let response = fixture.response("GET", "bytes=0-").await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    fixture.switch_away();
    assert!(fixture.session.upgrade().is_some());
    drop(fixture.resolve_source());

    assert_eq!(
        to_bytes(response.into_body(), 10).await.unwrap().as_ref(),
        b"0123456789"
    );
    // The capability remains registered; only an active response retains a session.
    assert!(fixture.session.upgrade().is_none());
    let reopened = fixture.resolve_source();
    assert_eq!(reopened.meta.id, fixture.first.id);
    assert_eq!(
        reopened
            .read_db()
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn dropping_an_unread_download_releases_its_session() {
    let fixture = Fixture::new();
    let response = fixture.response("GET", "bytes=2-5").await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    fixture.switch_away();
    assert!(fixture.session.upgrade().is_some());
    drop(response);
    assert!(fixture.session.upgrade().is_none());
    assert_eq!(fixture.resolve_source().meta.id, fixture.first.id);
}

#[tokio::test]
async fn overlapping_ranges_retain_the_session_until_the_last_response_is_dropped() {
    let fixture = Fixture::new();
    let first = fixture.response("GET", "bytes=0-4").await;
    let second = fixture.response("GET", "bytes=5-9").await;
    assert_eq!(first.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(second.status(), StatusCode::PARTIAL_CONTENT);
    fixture.switch_away();
    assert_eq!(
        to_bytes(first.into_body(), 5).await.unwrap().as_ref(),
        b"01234"
    );
    assert!(fixture.session.upgrade().is_some());
    drop(fixture.resolve_source());
    drop(second);
    assert!(fixture.session.upgrade().is_none());
    assert_eq!(fixture.resolve_source().meta.id, fixture.first.id);
}

#[tokio::test]
async fn current_session_without_a_cache_entry_remains_reusable_during_a_download() {
    for explicit in [false, true] {
        let mut fixture = Fixture::new();
        // Recovery can restore the active slot without populating the weak cache.
        *fixture.state.session.write().unwrap() = None;
        fixture.state.session_cache.lock().unwrap().clear();
        assert!(fixture.session.upgrade().is_none());
        let restored = Arc::new(library::open_session_light(fixture.first.clone()).unwrap());
        fixture.session = Arc::downgrade(&restored);
        *fixture.state.session.write().unwrap() = Some(restored);
        let current = if explicit {
            services::sessions::resolve(&fixture.state, Some(&fixture.first.id)).unwrap()
        } else {
            services::sessions::current(&fixture.state).unwrap()
        };
        assert!(Arc::ptr_eq(&current, &fixture.session.upgrade().unwrap()));
        drop(current);
        assert!(fixture.state.session_cache.lock().unwrap().is_empty());
        let response = fixture.response("GET", "bytes=0-").await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        fixture.switch_away();
        let resolved = fixture.resolve_source();
        assert!(Arc::ptr_eq(&resolved, &fixture.session.upgrade().unwrap()));
        drop(resolved);
        drop(response);
        assert!(fixture.session.upgrade().is_none());
    }
}

#[tokio::test]
async fn head_and_invalid_ranges_do_not_retain_a_session() {
    for (method, range, status) in [
        ("HEAD", "bytes=0-4", StatusCode::PARTIAL_CONTENT),
        ("GET", "bytes=100-200", StatusCode::RANGE_NOT_SATISFIABLE),
    ] {
        let fixture = Fixture::new();
        let response = fixture.response(method, range).await;
        assert_eq!(response.status(), status);
        fixture.switch_away();
        assert!(fixture.session.upgrade().is_none());
        assert_eq!(fixture.resolve_source().meta.id, fixture.first.id);
        drop(response);
    }
}

#[tokio::test]
async fn unread_download_blocks_migration_until_the_response_is_dropped() {
    let fixture = Fixture::new();
    let response = fixture.response("GET", "bytes=0-").await;
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    fixture.switch_away();
    let target = fixture.directory.path().join("migrated");
    let result = library::migrate_library(
        &mut fixture.state.config.write().unwrap(),
        &fixture.first.id,
        target.to_str().unwrap(),
    );
    assert_eq!(result.unwrap_err().code(), "operation_busy");
    assert!(std::path::Path::new(&fixture.first.path).is_dir());
    assert!(!target.exists());
    assert!(fixture
        .state
        .config
        .read()
        .unwrap()
        .pending_migrations
        .is_empty());
    drop(fixture.resolve_source());
    drop(response);
    assert!(fixture.session.upgrade().is_none());
    library::migrate_library(
        &mut fixture.state.config.write().unwrap(),
        &fixture.first.id,
        target.to_str().unwrap(),
    )
    .unwrap();
    assert!(!std::path::Path::new(&fixture.first.path).exists());
    assert_eq!(
        std::fs::read(target.join("images/download.bin")).unwrap(),
        b"0123456789"
    );
    assert_eq!(fixture.resolve_source().root, target);
}
