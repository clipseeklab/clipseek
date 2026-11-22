//! Library merge, cross-library asset moves and directory migration commands.

use super::library_switch_blocking;
use crate::error::{AppError, AppResult};
use crate::{library, AppState};
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, State};

/// Merge source into target in the background, removing the source after complete success.
#[tauri::command(async)]
pub async fn library_merge(
    app: AppHandle,
    state: State<'_, AppState>,
    source_id: String,
    target_id: String,
) -> AppResult<crate::transfer::TransferSummary> {
    {
        let cfg = state.config.read().unwrap();
        for id in [&source_id, &target_id] {
            if let Some(meta) = cfg.library_by_id(id) {
                library::check_compatible(std::path::Path::new(&meta.path))?;
            }
        }
    }
    if state
        .operation_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::OperationBusy);
    }
    struct Reset<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(&state.operation_running);
    let (source_is_active, target_is_active, source_meta) = {
        let cfg = state.config.read().unwrap();
        (
            cfg.active_library_id.as_deref() == Some(&source_id),
            cfg.active_library_id.as_deref() == Some(&target_id),
            cfg.library_by_id(&source_id).cloned(),
        )
    };
    // Flush first, then release the source handle after acquiring exclusive access. Reopen
    // its original location on failure to preserve the working context.
    if source_is_active {
        if let Some(session) = state.session.read().unwrap().clone() {
            {
                let conn = session.db.lock().unwrap();
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            }
            if let Some(shard) = session.vector.lock().unwrap().as_ref() {
                shard.flush().map_err(|e| {
                    AppError::Msg(format!("failed to save search index before merge: {e}"))
                })?;
            }
        }
    }
    let app2 = app.clone();
    let tid = target_id.clone();
    let mut open_sessions = std::collections::HashMap::new();
    if let Some(cur) = state.session.read().unwrap().clone() {
        if !source_is_active {
            open_sessions.insert(cur.meta.id.clone(), cur);
        }
    }
    let result = tokio::task::spawn_blocking(move || {
        let st = app2.state::<AppState>();
        let mut guard = st.config.write().unwrap();
        crate::transfer::merge_library_with_progress_and_close(
            &mut guard,
            &source_id,
            &tid,
            &open_sessions,
            &|progress| {
                let _ = app2.emit("operation://progress", progress);
            },
            || {
                if source_is_active {
                    *st.session.write().unwrap() = None;
                }
            },
        )
    })
    .await;
    let result = result
        .map_err(|e| AppError::Msg(format!("merge thread failed: {e}")))
        .and_then(|inner| inner);
    if result.is_err() && source_is_active && state.session.read().unwrap().is_none() {
        if let Some(meta) = source_meta {
            if let Ok(session) = library::open_session(meta) {
                let session = std::sync::Arc::new(session);
                let _ = crate::allow_session_assets(&app, &session);
                *state.session.write().unwrap() = Some(session);
            }
        }
    }
    let summary = result?;

    // Refresh the target if already active; switch to the merged target when the source was
    // active.
    if source_is_active || target_is_active {
        let _ = library_switch_blocking(app.clone(), &app.state::<AppState>(), target_id.clone());
    }
    Ok(summary)
}

/// Transfer assets to another library, then remove source records after a safe destination
/// commit.
#[tauri::command(async)]
pub async fn asset_move_to_library(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<i64>,
    target_library_id: String,
    context: crate::commands::body::LibraryContext,
) -> AppResult<usize> {
    if state
        .operation_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::OperationBusy);
    }
    struct Reset<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(&state.operation_running);
    let source = crate::services::sessions::resolve(&state, context.0.as_deref())?;
    let app2 = app.clone();
    tokio::task::spawn_blocking(move || {
        let worker_state = app2.state::<AppState>();
        crate::transfer::move_assets_from_session_with_progress(
            &worker_state,
            source,
            &ids,
            &target_library_id,
            &|progress| {
                let _ = app2.emit("operation://progress", progress);
            },
        )
    })
    .await
    .map_err(|error| AppError::Msg(format!("move assets thread failed: {error}")))?
}

/// Relocate a library directory.
#[tauri::command(async)]
pub async fn library_migrate(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    new_path: String,
) -> AppResult<()> {
    crate::volume::ensure_local_volume(std::path::Path::new(&new_path))?;
    {
        let cfg = state.config.read().unwrap();
        if let Some(meta) = cfg.library_by_id(&id) {
            library::check_compatible(std::path::Path::new(&meta.path))?;
        }
    }
    if state
        .operation_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::OperationBusy);
    }
    struct Reset<'a>(&'a std::sync::atomic::AtomicBool);
    impl Drop for Reset<'_> {
        fn drop(&mut self) {
            self.0.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset(&state.operation_running);
    let is_active = {
        let cfg = state.config.read().unwrap();
        cfg.active_library_id.as_deref() == Some(&id)
    };
    // Close the active session before relocation because its database handles reference the
    // old path.
    if is_active {
        if let Some(session) = state.session.read().unwrap().clone() {
            {
                let conn = session.db.lock().unwrap();
                conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")?;
            }
            if let Some(shard) = session.vector.lock().unwrap().as_ref() {
                shard.flush().map_err(|e| {
                    AppError::Msg(format!("failed to save search index before migration: {e}"))
                })?;
            }
        }
    }
    let operation_id = crate::util::new_entity_uid();
    let operation_dir = state.config.read().unwrap().operations_dir()?;
    let initial_progress = crate::transfer::TransferProgress {
        operation_id: operation_id.clone(),
        operation_type: "migrate_library".into(),
        status: "planning".into(),
        phase: "checking_space".into(),
        total_items: 1,
        completed_items: 0,
        total_bytes: 0,
        completed_bytes: 0,
        current_item: None,
        failed: 0,
        skipped: 0,
        message: None,
    };
    initial_progress.save_in(&operation_dir)?;
    let _ = app.emit("operation://progress", &initial_progress);
    let app2 = app.clone();
    let worker_operation_id = operation_id.clone();
    let worker_operation_dir = operation_dir.clone();
    let worker_id = id.clone();
    let joined = tokio::task::spawn_blocking(move || {
        let worker_state = app2.state::<AppState>();
        let mut cfg = worker_state.config.write().unwrap();
        library::migrate_library_with_progress_and_close(
            &mut cfg,
            &worker_id,
            &new_path,
            &|completed, total, item| {
                let progress = crate::transfer::TransferProgress {
                    operation_id: worker_operation_id.clone(),
                    operation_type: "migrate_library".into(),
                    status: "running".into(),
                    phase: if total == 0 {
                        "moving_directory".into()
                    } else {
                        "copying_files".into()
                    },
                    total_items: 0,
                    completed_items: 0,
                    total_bytes: total,
                    completed_bytes: completed,
                    current_item: (!item.is_empty()).then(|| item.to_string()),
                    failed: 0,
                    skipped: 0,
                    message: None,
                };
                let _ = progress.save_in(&worker_operation_dir);
                let _ = app2.emit("operation://progress", progress);
            },
            || {
                if is_active {
                    *worker_state.session.write().unwrap() = None;
                }
            },
        )
    })
    .await;
    let result = joined
        .map_err(|error| AppError::Msg(format!("migration thread failed: {error}")))
        .and_then(|inner| inner);
    if let Err(error) = result {
        let failed = crate::transfer::TransferProgress {
            operation_id: operation_id.clone(),
            operation_type: "migrate_library".into(),
            status: "failed".into(),
            phase: "copying_files".into(),
            total_items: 1,
            completed_items: 0,
            total_bytes: 0,
            completed_bytes: 0,
            current_item: None,
            failed: 1,
            skipped: 0,
            message: Some(format!(
                "Relocation failed; the original library was retained. Check the destination directory and retry: {error}"
            )),
        };
        let _ = failed.save_in(&operation_dir);
        let _ = app.emit("operation://progress", &failed);
        // Preserve the original session before exclusive ownership; reopen the original
        // location only after closure.
        if is_active && state.session.read().unwrap().is_none() {
            if let Some(meta) = state.config.read().unwrap().library_by_id(&id).cloned() {
                if let Ok(session) = library::open_session(meta) {
                    let session = std::sync::Arc::new(session);
                    let _ = crate::allow_session_assets(&app, &session);
                    *state.session.write().unwrap() = Some(session);
                }
            }
        }
        return Err(error);
    }
    let meta = state.config.read().unwrap().library_by_id(&id).cloned();

    if is_active {
        if let Some(meta) = meta {
            let session = std::sync::Arc::new(library::open_session(meta.clone())?);
            crate::allow_session_assets(&app, &session)?;
            *state.session.write().unwrap() = Some(session);
            let _ = app.emit("library://switched", &meta);
        }
    }
    let completed = crate::transfer::TransferProgress {
        operation_id,
        operation_type: "migrate_library".into(),
        status: "completed".into(),
        phase: "completed".into(),
        total_items: 1,
        completed_items: 1,
        total_bytes: 0,
        completed_bytes: 0,
        current_item: None,
        failed: 0,
        skipped: 0,
        message: None,
    };
    completed.save_in(&operation_dir)?;
    let _ = app.emit("operation://progress", completed);
    Ok(())
}
