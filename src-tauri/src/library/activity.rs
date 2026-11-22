//! Short state locks coordinate library file writers and nonblocking lifecycle cleanup.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, Weak};

use super::LibrarySession;
use crate::error::{AppError, AppResult};

static ACTIVE: OnceLock<Mutex<HashMap<PathBuf, Weak<AssetActivity>>>> = OnceLock::new();

/// Sessions opened through different entry points still coordinate the same physical library.
pub(super) fn for_root(root: &Path) -> AppResult<Arc<AssetActivity>> {
    let root = root.canonicalize()?;
    let mut registry = ACTIVE.get_or_init(Default::default).lock().unwrap();
    registry.retain(|_, entry| entry.strong_count() > 0);
    if let Some(activity) = registry.get(&root).and_then(Weak::upgrade) {
        if !activity.0.lock().unwrap().retired {
            return Ok(activity);
        }
    }
    let activity = Arc::new(AssetActivity::default());
    registry.insert(root, Arc::downgrade(&activity));
    Ok(activity)
}

/// A live response may outlast the current-session slot and its local cache entry.
pub(crate) fn session_for_root(root: &Path) -> Option<Arc<LibrarySession>> {
    let root = root.canonicalize().ok()?;
    let activity = ACTIVE.get()?.lock().unwrap().get(&root)?.upgrade()?;
    let state = activity.0.lock().unwrap();
    if state.retired {
        return None;
    }
    state.session.upgrade()
}

#[derive(Default)]
pub(crate) struct AssetActivity(Mutex<State>);
#[derive(Default)]
struct State {
    workers: usize,
    cleaning: bool,
    retired: bool,
    recovery_required: bool,
    session: Weak<LibrarySession>,
}

pub(crate) struct ActivityGuard<'a> {
    activity: &'a AssetActivity,
    cleanup: bool,
}

impl AssetActivity {
    pub(crate) fn work(&self) -> AppResult<ActivityGuard<'_>> {
        let mut state = self.0.lock().unwrap();
        if state.cleaning || state.retired || state.recovery_required {
            return Err(AppError::OperationBusy);
        }
        state.workers += 1;
        Ok(ActivityGuard {
            activity: self,
            cleanup: false,
        })
    }

    pub(crate) fn cleanup(&self) -> AppResult<ActivityGuard<'_>> {
        let mut state = self.0.lock().unwrap();
        if state.cleaning || state.workers != 0 || state.retired {
            return Err(AppError::OperationBusy);
        }
        state.cleaning = true;
        Ok(ActivityGuard {
            activity: self,
            cleanup: true,
        })
    }
}

impl ActivityGuard<'_> {
    /// An old session must never recreate the source tree after migration.
    pub(crate) fn retire(&self) {
        assert!(self.cleanup);
        self.activity.0.lock().unwrap().retired = true;
    }
}

pub(crate) struct OwnedActivityGuard {
    activity: Arc<AssetActivity>,
    // A stream must keep the cached session reusable without creating an Arc cycle.
    _session: Option<Arc<LibrarySession>>,
}
impl Drop for OwnedActivityGuard {
    fn drop(&mut self) {
        self.activity.0.lock().unwrap().workers -= 1;
    }
}
impl AssetActivity {
    pub(crate) fn bind_session(&self, session: &Arc<LibrarySession>) {
        self.0.lock().unwrap().session = Arc::downgrade(session);
    }

    pub(crate) fn owned_work(self: &Arc<Self>) -> AppResult<OwnedActivityGuard> {
        let mut state = self.0.lock().unwrap();
        if state.cleaning || state.retired || state.recovery_required {
            return Err(AppError::OperationBusy);
        }
        state.workers += 1;
        Ok(OwnedActivityGuard {
            activity: self.clone(),
            _session: state.session.upgrade(),
        })
    }
    pub(crate) fn set_recovery_required(&self, required: bool) {
        self.0.lock().unwrap().recovery_required = required;
    }
}

/// Protocol readers have paths rather than session handles. Coordinate them with
/// the longest matching physical root; unregistered external files need no guard.
pub(crate) fn work_for_path(path: &Path) -> AppResult<Option<OwnedActivityGuard>> {
    let resolved = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let activity = ACTIVE
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .iter()
        .filter(|(root, _)| resolved.starts_with(root) || path.starts_with(root))
        .max_by_key(|(root, _)| root.components().count())
        .and_then(|(_, activity)| activity.upgrade());
    activity.map(|a| a.owned_work()).transpose()
}

impl Drop for ActivityGuard<'_> {
    fn drop(&mut self) {
        let mut state = self.activity.0.lock().unwrap();
        if self.cleanup {
            state.cleaning = false;
        } else {
            state.workers -= 1;
        }
    }
}

impl LibrarySession {
    pub(crate) fn begin_asset_work(&self, id: i64) -> AppResult<ActivityGuard<'_>> {
        let guard = self.asset_activity.work()?;
        self.ensure_asset_active(id)?;
        Ok(guard)
    }

    pub(crate) fn ensure_asset_active(&self, id: i64) -> AppResult<()> {
        let conn = self.read_db();
        let active: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND deleted_at IS NULL)",
            [id],
            |row| row.get(0),
        )?;
        if !active {
            return Err(AppError::coded(
                "index_cancelled",
                "Asset processing cancelled because the asset was deleted",
            ));
        }
        crate::db::asset_cleanup::ensure_not_pending(&conn, id)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/asset_activity.rs"]
mod tests;
