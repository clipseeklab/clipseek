//! Cancellation, retries and persisted import recovery commands.

use super::ImportRecoveryAccepted;
use crate::error::{AppError, AppResult};
use crate::services::{
    imports::{self, PathImport},
    sessions,
};
use crate::{import, AppState};
use serde::Serialize;
use std::{path::PathBuf, sync::atomic::Ordering};
use tauri::{AppHandle, Emitter, State};

/// Cancel import after the current file finishes processing.
#[tauri::command]
pub fn import_cancel(state: State<AppState>) -> AppResult<()> {
    state.import_ctl.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Report whether import work is active.
#[tauri::command]
pub fn import_running(state: State<AppState>) -> bool {
    state.import_ctl.running.load(Ordering::SeqCst)
}

/// Retry failed or interrupted assets and schedule embeddings. An empty ID list retries all
/// failed assets in the library.
#[tauri::command]
pub async fn import_retry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Option<Vec<i64>>,
) -> AppResult<ImportRetryResult> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        import_retry_blocking(worker_app, state, ids)
    })
    .await
}

pub(crate) fn import_retry_blocking<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    state: &AppState,
    ids: Option<Vec<i64>>,
) -> AppResult<ImportRetryResult> {
    let session = sessions::current(state)?;
    if state.operation_running.load(Ordering::SeqCst) {
        return Err(AppError::OperationBusy);
    }
    let ids = match ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => {
            // Collect failed and interrupted assets automatically. Intermediate states count
            // as interrupted only when no import is active, avoiding interference with live
            // work.
            let importing = state.import_ctl.running.load(Ordering::SeqCst);
            let conn = session.db.lock().unwrap();
            let sql = if importing {
                // Retry explicit failures only.
                "SELECT id FROM assets WHERE import_stage LIKE 'error%'"
            } else {
                // Include failures and intermediate states left by interrupted imports.
                "SELECT id FROM assets WHERE import_stage LIKE 'error%' \
                 OR import_stage IN ('pending','copied','probing','thumb','frames')"
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt
                .query_map([], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        }
    };
    if ids.is_empty() {
        return Ok(ImportRetryResult {
            retried: 0,
            failed: 0,
        });
    }

    let _permit = import::processing_permit::ProcessingPermit::claim(
        &state.import_ctl.running,
        &state.import_ctl.cancel,
    )?;
    let emitter = |a: &crate::db::model::Asset| {
        crate::import::emit_asset_event(&app, &session.meta.id, a);
    };
    let (ok, failed) = crate::import::reprocess_assets_base_cancellable(
        &session,
        &ids,
        &emitter,
        &state.import_ctl.cancel,
    );

    // Any successful retry schedules missing embeddings.
    if ok > 0 && !state.import_ctl.cancel.load(Ordering::SeqCst) {
        crate::indexing::spawn_worker(&app, session.clone());
        let _ = app.emit("import://maintenance-done", &session.meta.id);
    }
    Ok(ImportRetryResult {
        retried: ok,
        failed,
    })
}

/// Retry result.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRetryResult {
    pub retried: usize,
    pub failed: usize,
}

/// List persistent interrupted jobs awaiting a user decision in the current library.
#[tauri::command]
pub async fn import_recovery_list<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<Vec<crate::db::import_jobs::ImportJob>> {
    crate::commands::blocking(app, import_recovery_list_blocking).await
}

pub(crate) fn import_recovery_list_blocking(
    state: &AppState,
) -> AppResult<Vec<crate::db::import_jobs::ImportJob>> {
    let session = sessions::current(state)?;
    let conn = session.db.lock().unwrap();
    let mut jobs = crate::db::import_jobs::recoverable(&conn)?;
    for job in &mut jobs {
        if job.mode == "folders" && !job.paths.iter().any(|path| PathBuf::from(path).is_dir()) {
            job.mode = "paths".into();
        }
    }
    Ok(jobs)
}

/// Resume a persistent job; hash deduplication skips previously registered files.
#[tauri::command]
pub async fn import_recovery_resume(
    app: AppHandle,
    job_id: String,
) -> AppResult<ImportRecoveryAccepted> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        let session = sessions::current(state)?;
        let operation = job_id.chars().take(8).collect::<String>();
        let started = std::time::Instant::now();
        crate::logging::info(
            "import",
            "recovery_resume_started",
            &operation,
            &format!(
                "processing_busy={};operation_busy={}",
                state.import_ctl.running.load(Ordering::SeqCst),
                state.operation_running.load(Ordering::SeqCst)
            ),
        );
        let job = {
            let conn = session.db.lock().unwrap();
            crate::db::import_jobs::get(&conn, &job_id)?
        };
        let Some(job) = job else {
            // Another request may already have resumed or cleaned up this job. Keep repeated
            // recovery requests idempotent so stale dialogs do not show misleading errors.
            crate::logging::info(
                "import",
                "recovery_resume_already_resolved",
                &operation,
                &format!("elapsed_ms={}", started.elapsed().as_millis()),
            );
            return Ok(ImportRecoveryAccepted {
                queued: 0,
                already_resolved: true,
            });
        };
        let mode = job.mode.clone();
        let path_count = job.paths.len();
        let result = if job.mode == "folders" {
            imports::submit_folders(worker_app, state, job.paths, Some(job.id))
        } else {
            imports::submit(worker_app, state, PathImport { paths: job.paths, folder_id: job.folder_id, batch_id: Some(job.id), ..Default::default() })
        };
        match &result {
            Ok(accepted) => crate::logging::info(
                "import",
                "recovery_resume_accepted",
                &operation,
                &format!(
                    "mode={mode};paths={path_count};queued={};elapsed_ms={}",
                    accepted.queued,
                    started.elapsed().as_millis()
                ),
            ),
            Err(error) => crate::logging::error(
                "import",
                "recovery_resume_failed",
                &operation,
                &format!(
                    "mode={mode};paths={path_count};processing_busy={};operation_busy={};elapsed_ms={};message={error}",
                    state.import_ctl.running.load(Ordering::SeqCst),
                    state.operation_running.load(Ordering::SeqCst),
                    started.elapsed().as_millis()
                ),
            ),
        }
        result.map(|accepted| ImportRecoveryAccepted {
            queued: accepted.queued,
            already_resolved: false,
        })
    })
    .await
}

/// Abandon an interrupted job; startup maintenance still indexes assets already registered.
#[tauri::command]
pub async fn import_recovery_abandon<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    job_id: String,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        import_recovery_abandon_blocking(state, job_id)
    })
    .await
}

pub(crate) fn import_recovery_abandon_blocking(state: &AppState, job_id: String) -> AppResult<()> {
    let session = sessions::current(state)?;
    let conn = session.db.lock().unwrap();
    crate::db::import_jobs::abandon(&conn, &job_id)
}
