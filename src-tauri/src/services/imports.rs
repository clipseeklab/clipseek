//! Shared import submission, persisted intent and background processing.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use fs2::available_space;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::error::{AppError, AppResult};
use crate::import;
use crate::services::sessions;
use crate::AppState;

mod context;
mod folders;
pub use folders::submit as submit_folders;

/// Accepted import response.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportAccepted {
    pub queued: usize,
}

/// Owned input for path imports, including the lifetime of temporary source files.
#[derive(Default)]
pub struct PathImport {
    pub paths: Vec<String>,
    pub folder_id: Option<i64>,
    pub batch_id: Option<String>,
    pub mode: Option<import::ImportMode>,
    pub temporary_source: Option<tempfile::TempDir>,
    pub tags: Option<Vec<String>>,
    pub web_source: Option<WebSource>,
    pub display_names: HashMap<String, String>,
}

pub struct WebSource {
    pub url: String,
    pub title: String,
}

/// The import://done batch ID identifies the matching frontend queue entry.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportDone {
    library_id: String,
    batch_id: Option<String>,
    total: usize,
    done: usize,
    failed: usize,
    duplicates: usize,
}

fn done_payload(
    library_id: &str,
    batch_id: &Option<String>,
    s: &import::ImportSummary,
) -> ImportDone {
    ImportDone {
        library_id: library_id.to_owned(),
        batch_id: batch_id.clone(),
        total: s.total,
        done: s.done,
        failed: s.failed,
        duplicates: s.duplicates,
    }
}

/// Wait for processing and embedding to become idle, then claim the pipeline; return false if
/// cancelled while waiting.
fn wait_processing(running: &AtomicBool, cancel: &AtomicBool) -> bool {
    loop {
        if cancel.load(Ordering::SeqCst) {
            return false;
        }
        if running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            return true;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn enforce_disk_capacity(
    session: &crate::library::LibrarySession,
    files: &[PathBuf],
) -> AppResult<()> {
    let incoming_bytes = files.iter().try_fold(0_u64, |total, path| {
        Ok::<_, std::io::Error>(total.saturating_add(std::fs::metadata(path)?.len()))
    })?;
    let available_bytes = available_space(&session.root)?;
    if incoming_bytes > available_bytes {
        return Err(AppError::DiskSpaceInsufficient {
            incoming_bytes,
            available_bytes,
        });
    }
    Ok(())
}

pub fn submit<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: &AppState,
    request: PathImport,
) -> AppResult<ImportAccepted> {
    let session = sessions::current(state)?;
    submit_in_session(app, state, session, request)
}

pub(crate) fn submit_in_session<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: &AppState,
    session: std::sync::Arc<crate::library::LibrarySession>,
    request: PathImport,
) -> AppResult<ImportAccepted> {
    let mut request = request;
    try_submit_in_session(app, state, session, &mut request)
}

/// Keep temporary source ownership with the caller until preflight accepts the job.
pub(crate) fn try_submit_in_session<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: &AppState,
    session: std::sync::Arc<crate::library::LibrarySession>,
    request: &mut PathImport,
) -> AppResult<ImportAccepted> {
    if state.operation_running.load(Ordering::SeqCst)
        && !session.index_worker_running.load(Ordering::SeqCst)
    {
        return Err(AppError::OperationBusy);
    }
    let defaults = {
        let cfg = state.config.read().unwrap();
        crate::db::indexing::IndexPolicy {
            visual: cfg.settings.auto_visual_index,
            subtitles: cfg.settings.auto_subtitle_index,
        }
    };
    let intent = context::resolve(
        &session,
        request.batch_id.as_deref(),
        defaults,
        request.tags.clone(),
    )?;
    let policy = intent.policy;
    let tags = intent.tags;
    let job_id = request.batch_id.clone().unwrap_or_else(crate::util::new_id);
    let paths = &request.paths;
    let folder_id = request.folder_id;
    let path_bufs: Vec<PathBuf> = paths.iter().map(PathBuf::from).collect();
    let import_mode = request.mode.unwrap_or_else(|| {
        import::ImportMode::from_setting(&state.config.read().unwrap().settings.import_mode)
    });
    let preflight = (|| -> AppResult<_> {
        let scanned = import::scanner::scan(&path_bufs);

        context::save_paths(
            &session,
            context::Job {
                id: &job_id,
                mode: "paths",
                paths,
                folder: folder_id,
                policy,
                tags: tags.as_deref(),
            },
            &scanned,
        )?;
        // Reference imports need no full-source storage preflight because only small derived
        // files are created.
        if import_mode == import::ImportMode::Copy {
            enforce_disk_capacity(&session, &scanned)?;
        }
        Ok(scanned)
    })();
    let scanned = match preflight {
        Ok(scanned) => scanned,
        Err(AppError::OperationBusy) => return Err(AppError::OperationBusy),
        Err(error) => {
            let conn = session.db.lock().unwrap();
            let _ = crate::db::processing::finish_batch(
                &conn,
                &job_id,
                "failed",
                Some(&error.to_string()),
            );
            // A failed preflight must not leave an endlessly failing recoverable job before
            // registration.
            let _ = crate::db::import_jobs::finish(&conn, &job_id, "failed", 0, 0, 1, 0);
            return Err(error);
        }
    };
    // Authorize referenced sources individually for asset://.
    if import_mode == import::ImportMode::Reference {
        crate::allow_source_files(&app, &scanned);
    }

    let PathImport {
        temporary_source: temp_dir,
        web_source,
        display_names,
        ..
    } = std::mem::take(request);
    let cancel = state.import_ctl.cancel.clone();
    let running = state.import_ctl.running.clone();
    let queued = scanned.len();

    std::thread::spawn(move || {
        let sample_temp_dir = temp_dir;
        let mut owns_processing = false;
        let mut was_cancelled = false;
        // Register references and emit cards promptly without waiting for earlier batches.

        let register_started = std::time::Instant::now();
        let mut batch = import::register_files_with_events(
            &app,
            &session,
            &path_bufs,
            &cancel,
            crate::import::ImportBatchOptions {
                folder_id,
                mode: import_mode,
                policy,
                batch_id: &job_id,
            },
        );

        context::apply_tags(&session, &batch, tags.as_deref());
        if let Some(source) = web_source.as_ref() {
            let conn = session.db.lock().unwrap();
            for (asset_id, _) in &batch.pending {
                if let Err(error) =
                    crate::db::assets::set_web_source(&conn, *asset_id, &source.url, &source.title)
                {
                    crate::logging::warn(
                        "capture",
                        "source_apply_failed",
                        "-",
                        &format!("asset_id={asset_id};message={error}"),
                    );
                }
            }
        }
        if !display_names.is_empty() {
            let conn = session.db.lock().unwrap();
            for (asset_id, file_name) in &batch.pending {
                if let Some(display_name) = display_names.get(file_name) {
                    if let Err(error) =
                        crate::db::assets::set_display_name(&conn, *asset_id, Some(display_name))
                    {
                        crate::logging::warn(
                            "capture",
                            "title_apply_failed",
                            "-",
                            &format!("asset_id={asset_id};message={error}"),
                        );
                    }
                }
            }
        }
        // Release temporary import sources after registration when ownership has been handed
        // off safely.
        drop(sample_temp_dir);
        crate::logging::info(
            "import",
            "register_timing",
            "-",
            &format!(
                "files={};elapsed_ms={}",
                batch.summary.total,
                register_started.elapsed().as_millis()
            ),
        );
        // Base import and visual/subtitle indexing have separate durable tasks.
        // The done event hands off base import; indexing continues independently.
        if !batch.aborted && wait_processing(&running, &cancel) {
            owns_processing = true;
            let processing_started = std::time::Instant::now();
            loop {
                let more = import::process_pending_chunk_with_events(
                    &app,
                    &session,
                    folder_id,
                    &mut batch,
                    &cancel,
                    import::PROCESS_CHUNK_SIZE,
                );
                // Schedule indexing separately in recoverable background tasks. Ordered
                // import writes do not require a full-library reconciliation every 64 assets.
                if !more {
                    break;
                }
            }
            crate::logging::info(
                "import",
                "processing_timing",
                "-",
                &format!(
                    "files={};elapsed_ms={}",
                    batch.summary.total,
                    processing_started.elapsed().as_millis()
                ),
            );
            was_cancelled |= cancel.load(Ordering::SeqCst);
            cancel.store(false, Ordering::SeqCst);
        }
        let status = if was_cancelled
            || batch.aborted
            || !batch.pending.is_empty()
            || cancel.load(Ordering::Relaxed)
        {
            "interrupted"
        } else {
            "completed"
        };
        {
            let conn = session.db.lock().unwrap();
            let _ = crate::db::processing::finish_batch(
                &conn,
                &job_id,
                if status == "interrupted" {
                    "interrupted"
                } else {
                    "failed"
                },
                Some("Base import did not finish"),
            );
            let _ = crate::db::import_jobs::finish(
                &conn,
                &job_id,
                status,
                batch.summary.total,
                batch.summary.done,
                batch.summary.failed,
                batch.summary.duplicates,
            );
        }

        let persisted_batch_id = Some(job_id);
        let _ = app.emit(
            "import://done",
            done_payload(&session.meta.id, &persisted_batch_id, &batch.summary),
        );
        // Emit completion before the next batch's progress so delayed done events cannot
        // clear another batch's state.
        if owns_processing {
            running.store(false, Ordering::SeqCst);
        }
        crate::indexing::spawn_worker(&app, session.clone());
        // Index optimization is independent of base import.
        if status == "completed" {
            if let Err(e) = crate::vector::optimize_index(&session) {
                eprintln!("Vector index optimization failed: {e}");
            }
        }
    });

    Ok(ImportAccepted { queued })
}
