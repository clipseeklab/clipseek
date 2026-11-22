use std::sync::atomic::Ordering;

use tauri::{AppHandle, Emitter, Manager};

use crate::error::{AppError, AppResult};
use crate::library::VectorHealth;
use crate::vector::recovery::{self, RebuildPreflight, VectorStatus};
use crate::AppState;

fn rebuild_start_allowed(health: &VectorHealth) -> bool {
    !matches!(health, VectorHealth::Rebuilding { .. })
}

#[tauri::command]
pub async fn vector_status(app: AppHandle) -> AppResult<VectorStatus> {
    super::blocking(app, |state| {
        let session = crate::services::sessions::current(state)?;
        Ok(recovery::status(&session))
    })
    .await
}

#[tauri::command]
pub async fn vector_retry_open(app: AppHandle) -> AppResult<VectorStatus> {
    super::blocking(app, |state| {
        let session = crate::services::sessions::current(state)?;
        recovery::retry_open(&session)
    })
    .await
}

#[tauri::command]
pub async fn vector_rebuild_preflight(app: AppHandle) -> AppResult<RebuildPreflight> {
    super::blocking(app, |state| {
        let session = crate::services::sessions::current(state)?;
        let operation = session.meta.id.chars().take(8).collect::<String>();
        let started = std::time::Instant::now();
        crate::logging::info("vector", "rebuild_preflight_started", &operation, "");
        match recovery::preflight(&session) {
            Ok(preflight) => {
                crate::logging::info(
                    "vector",
                    "rebuild_preflight_succeeded",
                    &operation,
                    &format!(
                        "elapsed_ms={};frames={};current_index_bytes={};estimated_required_bytes={};available_bytes={};can_start={}",
                        started.elapsed().as_millis(),
                        preflight.frame_count,
                        preflight.current_index_bytes,
                        preflight.estimated_required_bytes,
                        preflight.available_bytes,
                        preflight.can_start,
                    ),
                );
                Ok(preflight)
            }
            Err(error) => {
                crate::logging::error(
                    "vector",
                    "rebuild_preflight_failed",
                    &operation,
                    &format!(
                        "elapsed_ms={};message={error}",
                        started.elapsed().as_millis()
                    ),
                );
                Err(error)
            }
        }
    })
    .await
}

#[tauri::command]
pub async fn vector_rebuild_start(app: tauri::AppHandle, confirmed: bool) -> AppResult<String> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |_state| {
        vector_rebuild_start_blocking(worker_app, confirmed)
    })
    .await
}

pub(crate) fn vector_rebuild_start_blocking(app: AppHandle, confirmed: bool) -> AppResult<String> {
    if !confirmed {
        return Err(AppError::coded(
            "vector_rebuild_confirmation_required",
            "explicit user confirmation is required",
        ));
    }
    let state = app.state::<AppState>();
    let session = crate::services::sessions::current(&state)?;
    if !rebuild_start_allowed(&session.vector_health.lock().unwrap()) {
        return Err(AppError::coded(
            "vector_rebuild_already_running",
            "vector index rebuild is already running",
        ));
    }
    let preflight = recovery::preflight(&session)?;
    if !preflight.can_start {
        return Err(AppError::coded(
            "vector_rebuild_insufficient_space",
            "not enough free space to rebuild vector index",
        ));
    }
    if state
        .operation_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(AppError::OperationBusy);
    }
    if state
        .import_ctl
        .running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        state.operation_running.store(false, Ordering::SeqCst);
        return Err(AppError::OperationBusy);
    }
    let rebuild_id = uuid::Uuid::new_v4().simple().to_string();
    crate::logging::info(
        "vector",
        "rebuild_started",
        &rebuild_id,
        &format!(
            "library={};frames={};current_index_bytes={};estimated_required_bytes={};available_bytes={};user_confirmed=true",
            session.meta.id.chars().take(8).collect::<String>(),
            preflight.frame_count,
            preflight.current_index_bytes,
            preflight.estimated_required_bytes,
            preflight.available_bytes,
        ),
    );
    *session.vector_health.lock().unwrap() = VectorHealth::Rebuilding {
        rebuild_id: rebuild_id.clone(),
    };
    session.vector_rebuild_cancel.store(false, Ordering::SeqCst);
    let _ = app.emit("vector://status", recovery::status(&session));
    let thread_id = rebuild_id.clone();
    std::thread::spawn(move || {
        let state = app.state::<AppState>();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            state.clip.get_or_init().and_then(|clip| {
                recovery::rebuild(&session, &clip, &thread_id, &|progress| {
                    let _ = app.emit("vector://rebuild-progress", progress);
                })
            })
        }))
        .unwrap_or_else(|_| {
            Err(AppError::coded(
                "vector_rebuild_failed",
                "vector rebuild worker panicked",
            ))
        });
        match result {
            Ok(report) => {
                *session.vector_health.lock().unwrap() = VectorHealth::Ready;
                crate::logging::info(
                    "vector",
                    "rebuild_succeeded",
                    &thread_id,
                    &format!(
                        "indexed_frames={};failed_frames={}",
                        report.indexed_frames, report.failed_frames
                    ),
                );
                let mut done_status = recovery::status(&session);
                done_status.failed_frames = Some(report.failed_frames);
                let _ = app.emit("vector://rebuild-done", done_status);
            }
            Err(error) => {
                let message = error.to_string();
                // A manual rebuild may start while the current index is healthy. Most failures
                // happen before the directory swap, so keep that old in-memory index available.
                // If the swap had already released it, retry the journal-backed restore/open path.
                if session.vector.lock().unwrap().is_none() {
                    let _ = recovery::retry_open(&session);
                }
                if session.vector.lock().unwrap().is_some() {
                    *session.vector_health.lock().unwrap() = VectorHealth::Ready;
                } else {
                    *session.vector_health.lock().unwrap() = VectorHealth::RebuildPending {
                        code: "vector_rebuild_failed".into(),
                        message: message.clone(),
                    };
                }
                crate::logging::error(
                    "vector",
                    "rebuild_failed",
                    &thread_id,
                    &format!("message={message}"),
                );
                let _ = app.emit("vector://rebuild-failed", recovery::status(&session));
            }
        }
        state.operation_running.store(false, Ordering::SeqCst);
        state.import_ctl.running.store(false, Ordering::SeqCst);
        let _ = app.emit("vector://status", recovery::status(&session));
    });
    Ok(rebuild_id)
}

#[tauri::command]
pub async fn vector_rebuild_cancel<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<()> {
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        session.vector_rebuild_cancel.store(true, Ordering::SeqCst);
        Ok(())
    })
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/commands_vector.rs"]
mod tests;
