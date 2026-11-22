use tauri::{AppHandle, Emitter};

use crate::config::LibraryMeta;
use crate::error::{AppError, AppResult};
use crate::library;
use crate::AppState;
use std::sync::atomic::Ordering;

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryVolumeWarning {
    pub library_id: String,
    pub kind: String,
}

pub(crate) fn ensure_library_folder_icon(meta: &LibraryMeta) {
    if let Err(error) = library::install_library_folder_icon(std::path::Path::new(&meta.path)) {
        crate::logging::warn(
            "library",
            "folder_icon_install_failed",
            &meta.id.chars().take(8).collect::<String>(),
            &format!("message={error}"),
        );
    }
}

use crate::services::sessions::open_cached_session;

/// Existing libraries on network volumes can open with a migration notice because SQLite WAL
/// and vector locks are unreliable there.
pub(crate) fn warn_if_network_volume<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &library::LibrarySession,
) {
    if !crate::volume::is_network_volume(&session.root) {
        return;
    }
    let kind = crate::volume::volume_type_name(&session.root).unwrap_or_else(|| "network".into());
    crate::logging::warn(
        "library",
        "network_volume",
        "-",
        &format!("root_kind={kind}"),
    );
    let _ = app.emit("library://volume-warning", &session.meta);
}

/// Read the current library's network-volume status after startup; setup events may precede
/// listeners.
#[tauri::command]
pub async fn library_volume_warning<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<Option<LibraryVolumeWarning>> {
    crate::commands::blocking(app, move |state| Ok(library_volume_warning_blocking(state))).await
}

pub(crate) fn library_volume_warning_blocking(state: &AppState) -> Option<LibraryVolumeWarning> {
    let session = state.session.read().unwrap().clone()?;
    if !crate::volume::is_network_volume(&session.root) {
        return None;
    }
    Some(LibraryVolumeWarning {
        library_id: session.meta.id.clone(),
        kind: crate::volume::volume_type_name(&session.root).unwrap_or_else(|| "network".into()),
    })
}

/// List registered libraries with asset counts.
#[tauri::command]
pub async fn library_list<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<Vec<LibraryMeta>> {
    crate::commands::blocking(app, library_list_blocking).await
}

pub(crate) fn library_list_blocking(state: &AppState) -> AppResult<Vec<LibraryMeta>> {
    let mut cfg = state.config.write().unwrap();
    cfg.ensure_loaded()?;
    Ok(cfg.libraries.clone())
}

/// Return the active library.
#[tauri::command]
pub async fn library_current<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<Option<LibraryMeta>> {
    crate::commands::blocking(app, library_current_blocking).await
}

pub(crate) fn library_current_blocking(state: &AppState) -> AppResult<Option<LibraryMeta>> {
    crate::library_startup::wait_for_initial_session();
    let mut cfg = state.config.write().unwrap();
    cfg.ensure_loaded()?;
    let cur = cfg
        .active_library_id
        .as_deref()
        .and_then(|id| cfg.library_by_id(id).cloned());
    Ok(cur)
}

/// Return persistent library operations requiring attention after an application restart.
#[tauri::command]
pub async fn library_operations<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<Vec<crate::transfer::TransferProgress>> {
    crate::commands::blocking(app, move |state| Ok(library_operations_blocking(state))).await
}

pub(crate) fn library_operations_blocking(
    state: &AppState,
) -> Vec<crate::transfer::TransferProgress> {
    let (libraries, global_operations) = state
        .config
        .read()
        .map(|config| (config.libraries.clone(), config.operations_dir().ok()))
        .unwrap_or_default();
    let task_is_running = state.operation_running.load(Ordering::SeqCst);
    let mut out = Vec::new();
    let mut directories: Vec<_> = libraries
        .into_iter()
        .map(|library| std::path::Path::new(&library.path).join(".clipstudio-open/operations"))
        .collect();
    if let Some(dir) = global_operations {
        directories.push(dir);
    }
    let mut seen = std::collections::HashSet::new();
    for dir in directories {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Ok(raw) = std::fs::read_to_string(entry.path()) {
                if let Ok(mut progress) =
                    serde_json::from_str::<crate::transfer::TransferProgress>(&raw)
                {
                    if !seen.insert(progress.operation_id.clone()) {
                        continue;
                    }
                    if !task_is_running
                        && matches!(progress.status.as_str(), "planning" | "running")
                    {
                        progress.status = "failed".into();
                        progress.message = Some("The app exited before the operation completed. Original data was retained; check library locations and free space before retrying.".into());
                        let _ = progress
                            .save_in(entry.path().parent().unwrap_or(std::path::Path::new(".")));
                    }
                    if progress.status != "completed" {
                        let modified = entry.metadata().and_then(|m| m.modified()).ok();
                        out.push((modified, progress));
                    }
                }
            }
        }
    }
    out.sort_by_key(|(modified, _)| *modified);
    out.into_iter().map(|(_, progress)| progress).collect()
}

/// Create and activate a library.
#[tauri::command]
pub async fn library_create(
    app: tauri::AppHandle,
    name: String,
    path: String,
    color: Option<String>,
) -> AppResult<LibraryMeta> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        library_create_blocking(worker_app, state, name, path, color)
    })
    .await
}

pub(crate) fn library_create_blocking(
    app: AppHandle,
    state: &AppState,
    name: String,
    path: String,
    color: Option<String>,
) -> AppResult<LibraryMeta> {
    crate::volume::ensure_local_volume(std::path::Path::new(&path))?;
    let _switch_guard = state.session_switch_lock.lock().unwrap();
    let mut cfg = state.config.write().unwrap();
    let meta = library::create_library(&mut cfg, &name, &path, color.as_deref())?;
    drop(cfg);
    ensure_library_folder_icon(&meta);

    // Open the new library session.
    let session = open_cached_session(state, meta.clone())?;
    crate::allow_session_assets(&app, &session)?;
    *state.session.write().unwrap() = Some(session.clone());

    crate::services::maintenance::spawn_library_maintenance(&app, session);
    let _ = app.emit("library://switched", &meta);
    Ok(meta)
}

/// Register an existing library from its directory or metadata file.
#[tauri::command]
pub async fn library_import(app: tauri::AppHandle, path: String) -> AppResult<LibraryMeta> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        library_import_blocking(worker_app, state, path)
    })
    .await
}

pub(crate) fn library_import_blocking(
    app: AppHandle,
    state: &AppState,
    path: String,
) -> AppResult<LibraryMeta> {
    crate::volume::ensure_local_volume(std::path::Path::new(&path))?;
    let _switch_guard = state.session_switch_lock.lock().unwrap();
    let mut cfg = state.config.write().unwrap();
    let meta = library::import_library(&mut cfg, &path)?;
    drop(cfg);
    ensure_library_folder_icon(&meta);
    let session = open_cached_session(state, meta.clone())?;
    crate::allow_session_assets(&app, &session)?;
    *state.session.write().unwrap() = Some(session.clone());

    warn_if_network_volume(&app, &session);
    crate::services::maintenance::spawn_library_maintenance(&app, session);
    let _ = app.emit("library://switched", &meta);
    Ok(meta)
}

/// Switch the active library.
#[tauri::command]
pub async fn library_switch(app: tauri::AppHandle, id: String) -> AppResult<LibraryMeta> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        library_switch_blocking(worker_app, state, id)
    })
    .await
}

pub(crate) fn library_switch_blocking(
    app: AppHandle,
    state: &AppState,
    id: String,
) -> AppResult<LibraryMeta> {
    let _switch_guard = state.session_switch_lock.lock().unwrap();
    if let Some(current) = state.session.read().unwrap().as_ref() {
        if current.meta.id == id {
            ensure_library_folder_icon(&current.meta);
            crate::allow_session_assets(&app, current)?;
            warn_if_network_volume(&app, current);
            return Ok(current.meta.clone());
        }
    }
    let (meta,) = {
        let cfg = state.config.read().unwrap();
        let meta = cfg
            .library_by_id(&id)
            .cloned()
            .ok_or_else(|| AppError::LibraryNotFound)?;
        (meta,)
    };
    ensure_library_folder_icon(&meta);
    let session = open_cached_session(state, meta.clone())?;
    crate::allow_session_assets(&app, &session)?;
    let mut saved = state.config.write().unwrap();
    saved.ensure_loaded()?;
    let mut cfg = saved.clone();
    cfg.active_library_id = Some(id);
    cfg.save()?;
    *saved = cfg;
    *state.session.write().unwrap() = Some(session.clone());

    drop(saved);
    warn_if_network_volume(&app, &session);
    crate::services::maintenance::spawn_library_maintenance(&app, session);

    let _ = app.emit("library://switched", &meta);
    Ok(meta)
}

/// Validate a reselected registered directory and restore access. macOS authorization may not
/// survive a process restart; accept only the original directory without changing its
/// registration.
#[tauri::command]
pub async fn library_reauthorize<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    path: String,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        library_reauthorize_blocking(state, id, path)
    })
    .await
}

pub(crate) fn library_reauthorize_blocking(
    state: &AppState,
    id: String,
    path: String,
) -> AppResult<()> {
    let expected = {
        let cfg = state.config.read().unwrap();
        cfg.library_by_id(&id)
            .cloned()
            .ok_or_else(|| AppError::LibraryNotFound)?
    };
    let selected = std::fs::canonicalize(&path).map_err(|error| {
        AppError::coded(
            "library_reauthorize_failed",
            format!("cannot access selected directory: {error}"),
        )
    })?;
    let registered = std::fs::canonicalize(&expected.path).map_err(|error| {
        AppError::coded(
            "library_reauthorize_failed",
            format!(
                "cannot access registered directory {}: {error}",
                expected.path
            ),
        )
    })?;
    if selected != registered {
        return Err(AppError::coded(
            "library_reauthorize_wrong_directory",
            format!(
                "please select the registered library directory: {}",
                expected.path
            ),
        ));
    }

    let raw =
        std::fs::read_to_string(selected.join(library::LIBRARY_CONFIG_FILE)).map_err(|error| {
            AppError::coded(
                "library_reauthorize_failed",
                format!("cannot read library.json: {error}"),
            )
        })?;
    let selected_config: library::LibraryConfig = serde_json::from_str(&raw)?;
    if selected_config.id != id {
        return Err(AppError::coded(
            "library_reauthorize_wrong_directory",
            "the selected directory belongs to a different library",
        ));
    }

    // Validate read/write access immediately so authorization failures occur before switching
    // or merging.
    drop(crate::db::open_library_db(
        &library::LibraryDirs::new(&selected).db(),
    )?);
    Ok(())
}

/// Rename a library and optionally update its badge color.
#[tauri::command]
pub async fn library_rename<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    name: String,
    color: Option<String>,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        library_rename_blocking(state, id, name, color)
    })
    .await
}

pub(crate) fn library_rename_blocking(
    state: &AppState,
    id: String,
    name: String,
    color: Option<String>,
) -> AppResult<()> {
    let mut cfg = state.config.write().unwrap();
    let lib = cfg
        .libraries
        .iter_mut()
        .find(|l| l.id == id)
        .ok_or_else(|| AppError::LibraryNotFound)?;
    library::check_compatible(std::path::Path::new(&lib.path))?;
    lib.name = name.trim().to_string();
    if let Some(color) = color {
        lib.color = Some(color.trim().to_string());
    }
    library::write_library_config(std::path::Path::new(&lib.path), lib)?;
    cfg.save()?;
    Ok(())
}

/// Remove a library registration and optionally its managed directory. The currently open
/// library must be switched away from first.
#[tauri::command]
pub async fn library_delete<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: String,
    delete_files: bool,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        library_delete_blocking(state, id, delete_files)
    })
    .await
}

pub(crate) fn library_delete_blocking(
    state: &AppState,
    id: String,
    delete_files: bool,
) -> AppResult<()> {
    {
        let cfg = state.config.read().unwrap();
        if cfg.active_library_id.as_deref() == Some(&id) {
            return Err(AppError::LibraryState(
                "Cannot delete the active library; switch libraries first".into(),
            ));
        }
    }
    let mut cfg = state.config.write().unwrap();
    library::delete_library(&mut cfg, &id, delete_files)
}

mod transfer;
pub use transfer::*;
