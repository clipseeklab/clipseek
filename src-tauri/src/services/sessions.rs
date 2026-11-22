//! Session resolution shared by desktop commands, MCP and background work.
//! Explicit library selection never changes the UI's active library.

use std::sync::Arc;

use crate::config::LibraryMeta;
use crate::error::{AppError, AppResult};
use crate::{library, AppState};

/// Clone the active session without opening another library or changing the active selection.
pub fn current(state: &AppState) -> AppResult<Arc<library::LibrarySession>> {
    let session = state
        .session
        .read()
        .unwrap()
        .clone()
        .ok_or(AppError::LibraryNotOpen)?;
    Ok(register_file_session(session))
}

/// Only live file work keeps a strong session reference; registries remain weak.
fn register_file_session(session: Arc<library::LibrarySession>) -> Arc<library::LibrarySession> {
    session.asset_activity.bind_session(&session);
    session
}

/// Resolve an explicit library for non-UI callers without changing desktop selection.
pub fn resolve(
    state: &AppState,
    library_id: Option<&str>,
) -> AppResult<std::sync::Arc<library::LibrarySession>> {
    if library_id.is_none() {
        return current(state);
    }
    let id = library_id.unwrap();
    let active = state.session.read().unwrap().clone();
    if let Some(current) = active {
        if current.meta.id == id {
            return Ok(register_file_session(current));
        }
    }
    let _switch_guard = state.session_switch_lock.lock().unwrap();
    let meta = state
        .config
        .read()
        .unwrap()
        .library_by_id(id)
        .cloned()
        .ok_or(AppError::LibraryNotFound)?;
    open_cached_session(state, meta)
}

/// Reuse sessions still held by the page or background work to avoid duplicate WAL opens. The caller holds session_switch_lock shared with creation, registration, switching, and startup; validate compatibility before cache reuse.
pub(crate) fn open_cached_session(
    state: &AppState,
    meta: LibraryMeta,
) -> AppResult<std::sync::Arc<library::LibrarySession>> {
    library::check_compatible(std::path::Path::new(&meta.path))?;
    let cached = state
        .session_cache
        .lock()
        .unwrap()
        .get(&meta.id)
        .and_then(std::sync::Weak::upgrade)
        .or_else(|| library::activity::session_for_root(std::path::Path::new(&meta.path)));
    if let Some(session) = cached {
        if session.meta.id == meta.id && session.root.as_path() == std::path::Path::new(&meta.path)
        {
            return Ok(register_file_session(session));
        }
    }
    let session = register_file_session(Arc::new(library::open_session_light(meta)?));
    state
        .session_cache
        .lock()
        .unwrap()
        .insert(session.meta.id.clone(), Arc::downgrade(&session));
    Ok(session)
}
