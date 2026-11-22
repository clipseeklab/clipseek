use super::*;
use std::sync::{atomic::AtomicBool, Arc, Mutex, RwLock};

struct Fixture {
    _temp: tempfile::TempDir,
    state: crate::AppState,
    dst: crate::config::LibraryMeta,
    source: crate::db::model::Asset,
}

impl Fixture {
    fn new() -> Self {
        Self::with_mode(crate::import::ImportMode::Copy)
    }

    fn with_mode(mode: crate::import::ImportMode) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let input = temp.path().join("source.svg");
        fs::write(&input, r#"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="24"><rect width="32" height="24" fill="red"/></svg>"#).unwrap();
        let src = library::init_library(&temp.path().join("src"), "Source").unwrap();
        let dst = library::init_library(&temp.path().join("dst"), "Target").unwrap();
        let session = Arc::new(library::open_session(src.clone()).unwrap());
        assert_eq!(
            crate::import::run_import(
                &session,
                &[input],
                None,
                &AtomicBool::new(false),
                crate::import::ImportMode::Reference,
                &|_, _, _| {},
                &|_| {}
            )
            .done,
            1
        );
        if mode == crate::import::ImportMode::Copy {
            let conn = session.db.lock().unwrap();
            let (id, rel): (i64, String) = conn
                .query_row("SELECT id,rel_path FROM assets LIMIT 1", [], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .unwrap();
            let owned = session.root.join(rel);
            fs::create_dir_all(owned.parent().unwrap()).unwrap();
            fs::copy(temp.path().join("source.svg"), owned).unwrap();
            conn.execute(
                "UPDATE assets SET source_path=NULL,source_mtime=NULL WHERE id=?",
                [id],
            )
            .unwrap();
        }
        let source =
            db::assets::query_assets(&session.read_db(), &session.root, &Default::default())
                .unwrap()
                .items
                .remove(0);
        let mut config = GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
        config.libraries.extend([src.clone(), dst.clone()]);
        config.active_library_id = Some(src.id);
        let state = crate::AppState {
            mcp: Default::default(),
            config: RwLock::new(config),
            session: RwLock::new(Some(session)),
            session_cache: Mutex::new(HashMap::new()),
            session_switch_lock: Mutex::new(()),
            import_ctl: Default::default(),
            clip: Default::default(),
            operation_running: AtomicBool::new(false),
            source_check_running: Arc::new(AtomicBool::new(false)),
            source_check_cancel: Arc::new(AtomicBool::new(false)),
        };
        Self {
            _temp: temp,
            state,
            dst,
            source,
        }
    }

    fn target(&self, same_uid: bool) -> (LibrarySession, i64) {
        let src = crate::services::sessions::current(&self.state).unwrap();
        let dst = library::open_session(self.dst.clone()).unwrap();
        let id = copy_asset_to_session(&src, self.source.id, &dst, None).unwrap();
        if !same_uid {
            dst.db
                .lock()
                .unwrap()
                .execute(
                    "UPDATE assets SET uid=?1 WHERE id=?2",
                    rusqlite::params![util::new_entity_uid(), id],
                )
                .unwrap();
        }
        (dst, id)
    }
}

#[test]
fn transfer_refuses_trashed_duplicates_without_deleting_source() {
    for same_uid in [false, true] {
        let f = Fixture::new();
        let (dst, id) = f.target(same_uid);
        crate::services::assets::trash_assets(&dst, &[id]).unwrap();
        drop(dst);
        assert!(move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).is_err());
        assert!(Path::new(&f.source.abs_path).is_file());
        let src = crate::services::sessions::current(&f.state).unwrap();
        assert!(
            db::assets::get_asset(&src.read_db(), &src.root, f.source.id)
                .unwrap()
                .unwrap()
                .deleted_at
                .is_none()
        );
    }
}

#[test]
fn hash_duplicates_must_be_complete_before_source_deletion() {
    for missing in [false, true] {
        let f = Fixture::new();
        let (dst, id) = f.target(false);
        if missing {
            let target = db::assets::get_asset(&dst.read_db(), &dst.root, id)
                .unwrap()
                .unwrap();
            fs::remove_file(target.abs_path).unwrap();
        } else {
            db::assets::set_import_stage(&dst.db.lock().unwrap(), id, "error:interrupted").unwrap();
        }
        drop(dst);
        assert!(move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).is_err());
        assert!(Path::new(&f.source.abs_path).is_file());
    }
}

#[test]
fn moving_after_busy_source_cleanup_can_retry_the_original_request() {
    for reopen in [false, true] {
        let f = Fixture::new();
        let src = crate::services::sessions::current(&f.state).unwrap();
        let reader = src.asset_activity.work().unwrap();
        assert!(move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).is_err());
        assert!(db::asset_cleanup::pending(&src.read_db(), &f.source.uid).unwrap());
        drop(reader);
        if reopen {
            *f.state.session.write().unwrap() = None;
            let meta = src.meta.clone();
            drop(src);
            *f.state.session.write().unwrap() =
                Some(Arc::new(library::open_session(meta).unwrap()));
        }
        assert_eq!(
            move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).unwrap(),
            1
        );
        let src = crate::services::sessions::current(&f.state).unwrap();
        assert!(
            db::assets::get_asset(&src.read_db(), &src.root, f.source.id)
                .unwrap()
                .is_none()
        );
        let dst = library::open_session(f.dst.clone()).unwrap();
        let target =
            db::assets::query_assets(&dst.read_db(), &dst.root, &Default::default()).unwrap();
        assert_eq!(target.total, 1);
        assert!(Path::new(&target.items[0].abs_path).is_file());
    }
}

#[test]
fn copied_sources_are_not_deleted_for_missing_or_source_owned_reference_targets() {
    for missing in [false, true] {
        let f = Fixture::new();
        let (dst, id) = f.target(false);
        let target = db::assets::get_asset(&dst.read_db(), &dst.root, id)
            .unwrap()
            .unwrap();
        fs::remove_file(target.abs_path).unwrap();
        let source = if missing {
            f._temp.path().join("offline-original.svg")
        } else {
            std::path::PathBuf::from(&f.source.abs_path)
        };
        dst.db
            .lock()
            .unwrap()
            .execute(
                "UPDATE assets SET source_path=?1 WHERE id=?2",
                rusqlite::params![source.to_string_lossy(), id],
            )
            .unwrap();
        drop(dst);
        assert!(move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).is_err());
        assert!(Path::new(&f.source.abs_path).is_file());
    }
}

#[test]
fn independent_reference_targets_remain_reusable_and_offline_reference_links_survive() {
    for offline_reference in [false, true] {
        let mode = if offline_reference {
            crate::import::ImportMode::Reference
        } else {
            crate::import::ImportMode::Copy
        };
        let f = Fixture::with_mode(mode);
        let (dst, id) = f.target(false);
        let external = if offline_reference {
            let path = std::path::PathBuf::from(&f.source.abs_path);
            fs::remove_file(&path).unwrap();
            path
        } else {
            let path = f._temp.path().join("independent.svg");
            fs::copy(&f.source.abs_path, &path).unwrap();
            path
        };
        dst.db
            .lock()
            .unwrap()
            .execute(
                "UPDATE assets SET source_path=?1 WHERE id=?2",
                rusqlite::params![external.to_string_lossy(), id],
            )
            .unwrap();
        drop(dst);
        assert_eq!(
            move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).unwrap(),
            1
        );
        let dst = library::open_session(f.dst.clone()).unwrap();
        let target = db::assets::get_asset(&dst.read_db(), &dst.root, id)
            .unwrap()
            .unwrap();
        assert_eq!(target.storage, "reference");
        assert_eq!(target.abs_path, external.to_string_lossy());
        assert_eq!(external.is_file(), !offline_reference);
    }
}

#[test]
fn transfer_reuses_a_live_target_session_during_other_target_work() {
    let f = Fixture::new();
    let target = crate::services::sessions::resolve(&f.state, Some(&f.dst.id)).unwrap();
    let reader = target.asset_activity.work().unwrap();
    assert_eq!(
        move_assets_to_library(&f.state, &[f.source.id], &f.dst.id).unwrap(),
        1
    );
    assert_eq!(
        db::assets::query_assets(&target.read_db(), &target.root, &Default::default())
            .unwrap()
            .total,
        1
    );
    drop(reader);
}
