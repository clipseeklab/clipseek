//! Directory import planning, folder registration and background execution.

use super::{
    context, done_payload, enforce_disk_capacity, wait_processing, ImportAccepted, PathImport,
};
use crate::error::{AppError, AppResult};
use crate::services::{folders, sessions};
use crate::{import, AppState};
use std::{path::PathBuf, sync::atomic::Ordering};
use tauri::{AppHandle, Emitter};

pub fn submit(
    app: AppHandle,
    state: &AppState,
    paths: Vec<String>,
    batch_id: Option<String>,
) -> AppResult<ImportAccepted> {
    if state.operation_running.load(Ordering::SeqCst)
        && !state
            .session
            .read()
            .unwrap()
            .as_ref()
            .is_some_and(|s| s.index_worker_running.load(Ordering::SeqCst))
    {
        return Err(AppError::OperationBusy);
    }

    let session = sessions::current(state)?;
    let defaults = {
        let cfg = state.config.read().unwrap();
        crate::db::indexing::IndexPolicy {
            visual: cfg.settings.auto_visual_index,
            subtitles: cfg.settings.auto_subtitle_index,
        }
    };
    let intent = context::resolve(&session, batch_id.as_deref(), defaults, None)?;
    let policy = intent.policy;
    let job_id = batch_id.unwrap_or_else(crate::util::new_id);
    let import_mode =
        import::ImportMode::from_setting(&state.config.read().unwrap().settings.import_mode);
    {
        // Legacy folder-page jobs may contain ordinary file paths. Recover them as path
        // imports when no directory remains, safely removing stale paths.
        if !paths.iter().any(|path| PathBuf::from(path).is_dir()) {
            return super::submit(
                app,
                state,
                PathImport {
                    paths,
                    batch_id: Some(job_id),
                    ..Default::default()
                },
            );
        }
    }
    let prepare = || -> AppResult<_> {
        let mut inputs = Vec::new();
        let mut loose_files = Vec::new();
        for raw in &paths {
            let dir = PathBuf::from(raw);
            if dir.is_file() {
                loose_files.push(dir);
                continue;
            }
            if !dir.is_dir() {
                let conn = session.db.lock().unwrap();
                let record = crate::db::processing::register(&conn, &job_id, &dir, None)?;
                crate::db::processing::registered(
                    &conn,
                    record,
                    None,
                    "failed",
                    Some("Source directory is missing or inaccessible"),
                )?;
                continue;
            }
            if let Err(error) = std::fs::read_dir(&dir) {
                let conn = session.db.lock().unwrap();
                let record = crate::db::processing::register(&conn, &job_id, &dir, None)?;
                crate::db::processing::registered(
                    &conn,
                    record,
                    None,
                    "failed",
                    Some(&error.to_string()),
                )?;
                continue;
            }
            let raw_name = dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    AppError::coded("invalid_argument", "cannot read dropped folder name")
                })?;
            let name = folders::validate_folder_name(raw_name)?;
            inputs.push((dir, name));
        }
        // Create and announce logical folders before slow directory scans so newly dropped
        // folders appear immediately.
        let mut resolved_inputs = Vec::new();
        let mut resolved = std::collections::HashMap::<String, i64>::new();
        for (dir, name) in inputs {
            let key = name.to_lowercase();
            let folder_id = if let Some(id) = resolved.get(&key) {
                *id
            } else {
                let existing = {
                    let conn = session.db.lock().unwrap();
                    crate::db::folders::get_by_name(&conn, &name)?
                };
                let id = match existing {
                    Some(folder) => folder.id,
                    None => folders::create_folder(&session, &name)?.id,
                };
                resolved.insert(key, id);
                id
            };
            resolved_inputs.push((dir, folder_id));
        }
        let _ = app.emit("folders://changed", &session.meta.id);

        // Import immediate children of any number of dropped directories into matching
        // logical folders.
        let mut batches: Vec<_> = resolved_inputs
            .into_iter()
            .map(|(dir, folder_id)| (import::scanner::scan_shallow_dir(&dir), Some(folder_id)))
            .collect();
        if !loose_files.is_empty() {
            batches.push((import::scanner::scan(&loose_files), None));
        }
        let all_files: Vec<_> = batches
            .iter()
            .flat_map(|(files, _)| files.iter().cloned())
            .collect();

        {
            let mut conn = session.db.lock().unwrap();
            let tx = conn.transaction()?;
            for (files, folder) in &batches {
                for path in files {
                    crate::db::processing::register(&tx, &job_id, path, *folder)?;
                }
            }
            tx.commit()?;
        }
        // Reference imports need no full-source storage preflight because only small derived
        // files are created.
        if import_mode == import::ImportMode::Copy {
            enforce_disk_capacity(&session, &all_files)?;
        }
        if import_mode == import::ImportMode::Reference {
            crate::allow_source_files(&app, &all_files);
        }
        Ok(batches)
    };
    let batches = match prepare() {
        Ok(batches) => batches,
        Err(error) => {
            let conn = session.db.lock().unwrap();
            let _ = crate::db::processing::finish_batch(
                &conn,
                &job_id,
                "failed",
                Some(&error.to_string()),
            );
            let _ = crate::db::import_jobs::finish(&conn, &job_id, "failed", 0, 0, 1, 0);
            return Err(error);
        }
    };
    context::save(
        &session,
        context::Job {
            id: &job_id,
            mode: "folders",
            paths: &paths,
            folder: None,
            policy,
            tags: intent.tags.as_deref(),
        },
    )?;
    let queued = batches.iter().map(|(files, _)| files.len()).sum();
    let cancel = state.import_ctl.cancel.clone();
    let running = state.import_ctl.running.clone();

    std::thread::spawn(move || {
        let mut owns_processing = false;
        let mut was_cancelled = false;
        // Register all folders with per-file card events, then queue processing together.

        let register_started = std::time::Instant::now();
        let mut combined = import::ImportSummary::default();
        let mut registered: Vec<(Option<i64>, import::RegisteredBatch)> = Vec::new();
        let mut aborted = false;
        for (files, folder_id) in batches {
            if cancel.load(Ordering::Relaxed) {
                break;
            }
            let batch = import::register_files_with_events(
                &app,
                &session,
                &files,
                &cancel,
                crate::import::ImportBatchOptions {
                    folder_id,
                    mode: import_mode,
                    policy,
                    batch_id: &job_id,
                },
            );
            combined.total += batch.summary.total;
            combined.duplicates += batch.summary.duplicates;
            combined.failed += batch.summary.failed;
            aborted = aborted || batch.aborted;
            registered.push((folder_id, batch));
        }
        crate::logging::info(
            "import",
            "register_timing",
            "-",
            &format!(
                "files={};elapsed_ms={}",
                combined.total,
                register_started.elapsed().as_millis()
            ),
        );

        if !aborted && wait_processing(&running, &cancel) {
            owns_processing = true;
            let processing_started = std::time::Instant::now();
            for (folder_id, mut batch) in registered {
                loop {
                    let more = import::process_pending_chunk_with_events(
                        &app,
                        &session,
                        folder_id,
                        &mut batch,
                        &cancel,
                        import::PROCESS_CHUNK_SIZE,
                    );
                    // Indexing is a separately recoverable background task.
                    if !more {
                        break;
                    }
                }
                combined.done += batch.summary.done;
                combined.failed += batch.summary.failed;
            }
            crate::logging::info(
                "import",
                "processing_timing",
                "-",
                &format!(
                    "files={};elapsed_ms={}",
                    combined.total,
                    processing_started.elapsed().as_millis()
                ),
            );
            was_cancelled |= cancel.load(Ordering::SeqCst);
            cancel.store(false, Ordering::SeqCst);
        }
        let status = if was_cancelled || aborted || cancel.load(Ordering::Relaxed) {
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
                combined.total,
                combined.done,
                combined.failed,
                combined.duplicates,
            );
        }

        let persisted_batch_id = Some(job_id);
        let _ = app.emit(
            "import://done",
            done_payload(&session.meta.id, &persisted_batch_id, &combined),
        );
        if owns_processing {
            running.store(false, Ordering::SeqCst);
        }
        crate::indexing::spawn_worker(&app, session.clone());
        if status == "completed" {
            if let Err(e) = crate::vector::optimize_index(&session) {
                eprintln!("Vector index optimization failed: {e}");
            }
        }
    });

    Ok(ImportAccepted { queued })
}
