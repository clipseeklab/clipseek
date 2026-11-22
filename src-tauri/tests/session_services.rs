//! Session resolution must preserve UI state and reuse one live session per library path.

use std::collections::HashMap;
use std::fs;
use std::sync::{atomic::AtomicBool, Arc, Barrier, Mutex, RwLock};

use clip_studio_lib::config::{GlobalConfig, LibraryMeta};
use clip_studio_lib::error::AppResult;
use clip_studio_lib::library::{self, LibrarySession};
use clip_studio_lib::{clip, import, services, vector, AppState};
use serde_json::json;

struct Fixture {
    state: AppState,
    current: Arc<LibrarySession>,
    secondary: LibraryMeta,
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let first = library::init_library(&temp.path().join("first"), "First").unwrap();
        let secondary = library::init_library(&temp.path().join("second"), "Second").unwrap();
        let current = Arc::new(library::open_session(first.clone()).unwrap());
        let mut config = GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
        config.libraries = vec![first.clone(), secondary.clone()];
        config.active_library_id = Some(first.id);
        config.save().unwrap();
        let state = AppState {
            mcp: Default::default(),
            config: RwLock::new(config),
            session: RwLock::new(Some(current.clone())),
            session_cache: Mutex::new(HashMap::new()),
            session_switch_lock: Mutex::new(()),
            import_ctl: import::ImportControl::default(),
            clip: clip::ClipHandle::default(),
            operation_running: AtomicBool::new(false),
            source_check_running: Arc::new(AtomicBool::new(false)),
            source_check_cancel: Arc::new(AtomicBool::new(false)),
        };
        Self {
            state,
            current,
            secondary,
            temp,
        }
    }

    fn resolve_secondary(&self) -> Arc<LibrarySession> {
        services::session(&self.state, Some(&self.secondary.id)).unwrap()
    }

    fn assert_current_unchanged(&self) {
        assert!(Arc::ptr_eq(
            self.state.session.read().unwrap().as_ref().unwrap(),
            &self.current,
        ));
        assert_eq!(
            self.state
                .config
                .read()
                .unwrap()
                .active_library_id
                .as_deref(),
            Some(self.current.meta.id.as_str()),
        );
    }
}

fn assert_error(result: AppResult<Arc<LibrarySession>>, expected: &str) {
    assert_eq!(
        result.err().expect("session resolution must fail").code(),
        expected
    );
}

#[test]
fn current_resolution_preserves_arc_identity_without_using_the_cache() {
    let fixture = Fixture::new();
    for id in [None, Some(fixture.current.meta.id.as_str())] {
        let resolved = services::session(&fixture.state, id).unwrap();
        assert!(Arc::ptr_eq(&resolved, &fixture.current));
    }
    assert!(fixture.state.session_cache.lock().unwrap().is_empty());
    fixture.assert_current_unchanged();
}

#[test]
fn missing_current_session_and_unknown_ids_keep_distinct_errors() {
    let fixture = Fixture::new();
    *fixture.state.session.write().unwrap() = None;
    assert_error(services::session(&fixture.state, None), "library_not_open");
    for id in ["unknown", ""] {
        assert_error(
            services::session(&fixture.state, Some(id)),
            "library_not_found",
        );
    }
    assert!(fixture.state.session_cache.lock().unwrap().is_empty());
    let explicit = fixture.resolve_secondary();
    assert_eq!(explicit.meta.id, fixture.secondary.id);
    assert!(fixture.state.session.read().unwrap().is_none());
}

#[test]
fn explicit_resolution_reuses_a_live_session_without_writing_ui_configuration() {
    let fixture = Fixture::new();
    let config_path = fixture.temp.path().join("config.json");
    let original_config = fs::read(&config_path).unwrap();
    let first = fixture.resolve_secondary();
    let second = fixture.resolve_secondary();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(fs::read(config_path).unwrap(), original_config);
    fixture.assert_current_unchanged();
}

#[test]
fn concurrent_resolution_shares_the_database_and_vector_handle() {
    let fixture = Fixture::new();
    let barrier = Barrier::new(8);
    let sessions = std::thread::scope(|scope| {
        let handles = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    fixture.resolve_secondary()
                })
            })
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert!(sessions
        .iter()
        .all(|session| Arc::ptr_eq(session, &sessions[0])));
    sessions[0]
        .db
        .lock()
        .unwrap()
        .execute("CREATE TABLE session_probe(value INTEGER)", [])
        .unwrap();
    sessions[0]
        .db
        .lock()
        .unwrap()
        .execute("INSERT INTO session_probe VALUES(42)", [])
        .unwrap();
    let embedding = vec![1.0_f32; 768];
    vector::upsert_points(
        &sessions[0],
        vec![(1, embedding.clone(), json!({"asset_id": 1}))],
    )
    .unwrap();
    for session in &sessions {
        let value: i64 = session
            .read_db()
            .query_row("SELECT value FROM session_probe", [], |row| row.get(0))
            .unwrap();
        assert_eq!(value, 42);
        assert_eq!(
            vector::search(session, embedding.clone(), None, 1)
                .unwrap()
                .len(),
            1
        );
    }
    fixture.assert_current_unchanged();
}

#[test]
fn expired_weak_cache_entries_reopen_the_persisted_database() {
    let fixture = Fixture::new();
    let first = fixture.resolve_secondary();
    first
        .db
        .lock()
        .unwrap()
        .execute("INSERT INTO meta VALUES('session_probe', 'saved')", [])
        .unwrap();
    let weak = Arc::downgrade(&first);
    drop(first);
    assert!(weak.upgrade().is_none());
    let reopened = fixture.resolve_secondary();
    let value: String = reopened
        .read_db()
        .query_row(
            "SELECT value FROM meta WHERE key='session_probe'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(value, "saved");
    let cached = fixture
        .state
        .session_cache
        .lock()
        .unwrap()
        .get(&fixture.secondary.id)
        .unwrap()
        .upgrade()
        .unwrap();
    assert!(Arc::ptr_eq(&cached, &reopened));
}

#[test]
fn changed_registered_paths_do_not_reuse_the_previous_path_session() {
    let fixture = Fixture::new();
    let original = fixture.resolve_secondary();
    let replacement =
        library::init_library(&fixture.temp.path().join("replacement"), "Replacement").unwrap();
    let mut moved = fixture.secondary.clone();
    moved.path = replacement.path;
    library::write_library_config(std::path::Path::new(&moved.path), &moved).unwrap();
    fixture
        .state
        .config
        .write()
        .unwrap()
        .libraries
        .iter_mut()
        .find(|meta| meta.id == moved.id)
        .unwrap()
        .path = moved.path.clone();
    let resolved = fixture.resolve_secondary();
    assert!(!Arc::ptr_eq(&resolved, &original));
    assert_eq!(resolved.root, std::path::PathBuf::from(&moved.path));
    assert_eq!(
        original.root,
        std::path::PathBuf::from(&fixture.secondary.path)
    );
    fixture.assert_current_unchanged();
}

#[test]
fn compatibility_is_checked_before_reusing_a_cached_session() {
    let fixture = Fixture::new();
    let cached = fixture.resolve_secondary();
    cached
        .db
        .lock()
        .unwrap()
        .execute(
            "UPDATE meta SET value=?1 WHERE key='schema_version'",
            [clip_studio_lib::db::SCHEMA_VERSION + 1],
        )
        .unwrap();
    assert_error(
        services::session(&fixture.state, Some(&fixture.secondary.id)),
        "library_upgrade_required",
    );
    fixture.assert_current_unchanged();
}

#[test]
fn failed_open_releases_the_switch_lock_and_does_not_cache_a_partial_session() {
    let fixture = Fixture::new();
    fixture
        .state
        .config
        .write()
        .unwrap()
        .libraries
        .iter_mut()
        .find(|meta| meta.id == fixture.secondary.id)
        .unwrap()
        .path = fixture
        .temp
        .path()
        .join("missing")
        .to_string_lossy()
        .into_owned();
    assert_error(
        services::session(&fixture.state, Some(&fixture.secondary.id)),
        "library_dir_invalid",
    );
    assert!(!fixture
        .state
        .session_cache
        .lock()
        .unwrap()
        .contains_key(&fixture.secondary.id));
    fixture
        .state
        .config
        .write()
        .unwrap()
        .libraries
        .iter_mut()
        .find(|meta| meta.id == fixture.secondary.id)
        .unwrap()
        .path = fixture.secondary.path.clone();
    assert_eq!(fixture.resolve_secondary().meta.id, fixture.secondary.id);
    fixture.assert_current_unchanged();
}

#[test]
fn service_facade_and_domain_entry_points_share_sessions() {
    let fixture = Fixture::new();
    let facade_current = services::session(&fixture.state, None).unwrap();
    let current = services::sessions::current(&fixture.state).unwrap();
    assert!(Arc::ptr_eq(&facade_current, &current));
    let facade_explicit = services::session(&fixture.state, Some(&fixture.secondary.id)).unwrap();
    let resolved =
        services::sessions::resolve(&fixture.state, Some(&fixture.secondary.id)).unwrap();
    assert!(Arc::ptr_eq(&facade_explicit, &resolved));
    fixture.assert_current_unchanged();
}
