//! Reference source availability, batched checks and cancellation.

use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::AppState;
use rusqlite::params;
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, State};

pub(super) fn require_source(
    conn: &rusqlite::Connection,
    asset: &crate::db::model::Asset,
) -> AppResult<String> {
    if asset.storage != "reference" {
        return Ok(asset.abs_path.clone());
    }
    if std::path::Path::new(&asset.abs_path).is_file() {
        if asset.source_missing {
            let _ = db::assets::set_source_missing(conn, asset.id, false);
        }
        return Ok(asset.abs_path.clone());
    }
    if !asset.source_missing {
        let _ = db::assets::set_source_missing(conn, asset.id, true);
    }
    Err(AppError::coded(
        "source_file_missing",
        format!("source file is missing: {}", asset.abs_path),
    ))
}

/// Batch reference-source validation result.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCheckSummary {
    pub total: usize,
    pub missing: usize,
    pub cancelled: bool,
}

const SOURCE_CHECK_CHUNK: usize = 64;

/// Check reference sources in chunks, updating flags transactionally and emitting cancellable
/// source://check progress.
#[tauri::command]
pub async fn source_check(
    app: tauri::AppHandle,
    library_id: Option<String>,
) -> AppResult<SourceCheckSummary> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        // Reuse the matching current session or open the requested library explicitly,
        // independent of UI selection.
        let session = crate::services::sessions::resolve(state, library_id.as_deref())?;
        if state
            .source_check_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(AppError::TooManyOperations);
        }
        let running = state.source_check_running.clone();
        let cancel = state.source_check_cancel.clone();
        cancel.store(false, Ordering::SeqCst);

        struct RunningGuard(Arc<AtomicBool>);
        impl Drop for RunningGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _guard = RunningGuard(running);

        scan_reference_sources(&session, &cancel, &|done, total| {
            let _ = worker_app.emit(
                "source://check",
                serde_json::json!({ "done": done, "total": total }),
            );
        })
    })
    .await
}

/// Cancel an active source-file check.
#[tauri::command]
pub fn source_check_cancel(state: State<AppState>) {
    state.source_check_cancel.store(true, Ordering::SeqCst);
}

/// Stat reference paths in chunks, update source_missing transactionally, and report
/// progress. Stop promptly on cancellation and return cancelled=true.
pub fn scan_reference_sources(
    session: &LibrarySession,
    cancel: &AtomicBool,
    on_progress: &dyn Fn(usize, usize),
) -> AppResult<SourceCheckSummary> {
    let items = {
        let conn = session.db.lock().unwrap();
        crate::db::indexing::invalidate_changed_references(&conn)?;
        db::assets::reference_assets(&conn)?
    };
    let total = items.len();
    let mut missing_total = 0usize;
    let mut done = 0usize;
    let mut cancelled = false;

    for chunk in items.chunks(SOURCE_CHECK_CHUNK) {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            break;
        }
        // Use four concurrent stat workers to avoid serial network-volume latency.
        let results: Vec<(i64, bool)> = std::thread::scope(|scope| {
            let workers = chunk.len().clamp(1, 4);
            let size = chunk.len().div_ceil(workers);
            let handles: Vec<_> = chunk
                .chunks(size)
                .map(|part| {
                    scope.spawn(move || {
                        part.iter()
                            .map(|(id, path)| (*id, !Path::new(path).is_file()))
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|handle| handle.join().unwrap_or_default())
                .collect()
        });
        {
            let conn = session.db.lock().unwrap();
            let tx = conn.unchecked_transaction()?;
            for (id, missing) in &results {
                tx.execute(
                    "UPDATE assets SET source_missing = ?1 WHERE id = ?2",
                    params![*missing, *id],
                )?;
            }
            tx.commit()?;
        }
        missing_total += results.iter().filter(|(_, missing)| *missing).count();
        done += results.len();
        on_progress(done, total);
    }
    crate::logging::info(
        "source",
        "check_done",
        "-",
        &format!("total={total};missing={missing_total};cancelled={cancelled}"),
    );
    Ok(SourceCheckSummary {
        total,
        missing: missing_total,
        cancelled,
    })
}
