//! Background library recovery and embedding orchestration.
use crate::library::LibrarySession;
use crate::{vector, AppState};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// Synchronously reconcile, encode, and persist pending units with progress and asset events.
pub fn run_embedding<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<LibrarySession>,
    reconcile: bool,
) {
    let state = app.state::<AppState>();
    let operation = session.meta.id.chars().take(8).collect::<String>();
    let started = std::time::Instant::now();
    let vector_ready = session.vector.lock().unwrap().is_some();
    crate::logging::info(
        "import",
        "embedding_started",
        &operation,
        &format!("reconcile={reconcile};vector_ready={vector_ready}"),
    );
    if reconcile && session.vector.lock().unwrap().is_some() {
        let reconcile_started = std::time::Instant::now();
        match vector::reconcile_embedding_state(session) {
            Ok(n) if n > 0 => eprintln!("Detected {n} missing vectors; rebuilding automatically"),
            Ok(_) => {}
            Err(e) => {
                crate::logging::error(
                    "import",
                    "vector_reconcile_failed",
                    &operation,
                    &format!("message={e}"),
                );
                eprintln!("Vector consistency check failed: {e}");
                return;
            }
        }
        crate::logging::info(
            "import",
            "vector_reconcile_done",
            "-",
            &format!("elapsed_ms={}", reconcile_started.elapsed().as_millis()),
        );
    }
    let total_frames = {
        let conn = session.db.lock().unwrap();
        crate::db::assets::count_pending_frames(&conn)
            .unwrap_or(0)
            .max(0) as usize
    };
    let library_id = session.meta.id.clone();
    if total_frames > 0 {
        let clip = match state.clip.get_or_init() {
            Ok(c) => c,
            Err(e) => {
                crate::logging::error(
                    "import",
                    "clip_init_failed",
                    &operation,
                    &format!("message={e}"),
                );
                eprintln!("Encoder loading failed; skipping embeddings: {e}");
                return;
            }
        };
        let done_frames = std::cell::Cell::new(0usize);
        let emitter = |a: &crate::db::model::Asset| {
            done_frames.set(done_frames.get() + 1);
            crate::import::emit_embed_progress(
                app,
                &library_id,
                total_frames,
                done_frames.get(),
                &a.file_name,
            );
            crate::import::emit_asset_event(app, &library_id, a);
        };
        match vector::embed_pending(session, &clip, &emitter) {
            Ok(n) if n > 0 => eprintln!("Embedded {n} index units"),
            Ok(_) => {}
            Err(e) => {
                crate::logging::error(
                    "import",
                    "vector_embed_failed",
                    &operation,
                    &format!("pending_frames={total_frames};message={e}"),
                );
                eprintln!("Embedding failed: {e}");
            }
        }
    }
    crate::logging::info(
        "import",
        "embedding_finished",
        &operation,
        &format!(
            "pending_frames={total_frames};elapsed_ms={}",
            started.elapsed().as_millis()
        ),
    );
}

/// Recover embeddings after imports and session opens using a background worker.
pub fn spawn_embedding<R: tauri::Runtime>(app: &AppHandle<R>, session: Arc<LibrarySession>) {
    let app = app.clone();
    std::thread::spawn(move || {
        let operation = session.meta.id.chars().take(8).collect::<String>();
        run_embedding(&app, &session, true);
        if let Err(e) = vector::optimize_index(&session) {
            crate::logging::error(
                "import",
                "vector_optimize_failed",
                &operation,
                &format!("message={e}"),
            );
            eprintln!("Vector index optimization failed: {e}");
        }
        crate::indexing::spawn_worker(&app, session.clone());
        let _ = app.emit("import://maintenance-done", &session.meta.id);
    });
}

/// Rebuild legacy video/GIF inputs before normal vector reconciliation.
pub fn spawn_library_maintenance<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: Arc<LibrarySession>,
) {
    let app = app.clone();
    std::thread::spawn(move || {
        let operation = session.meta.id.chars().take(8).collect::<String>();
        let maintenance_started = std::time::Instant::now();
        crate::logging::info(
            "import",
            "maintenance_started",
            &operation,
            &format!("vector_ready={}", session.vector.lock().unwrap().is_some()),
        );
        // Share the import processing lock so registration can continue while media and index
        // mutation remain serialized with startup recovery.
        let running = app.state::<AppState>().import_ctl.running.clone();
        while running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            std::thread::sleep(Duration::from_millis(200));
        }
        struct RunningGuard(Arc<AtomicBool>);
        impl Drop for RunningGuard {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _running_guard = RunningGuard(running);

        let payload_upgrade_ids = {
            let conn = session.db.lock().unwrap();
            let version: i64 = conn
                .query_row(
                    "SELECT value FROM meta WHERE key = 'vector_payload_version'",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            if version < 2 {
                let mut stmt = conn.prepare("SELECT id FROM assets").ok();
                stmt.as_mut()
                    .and_then(|stmt| stmt.query_map([], |r| r.get(0)).ok())
                    .map(|rows| rows.filter_map(Result::ok).collect::<Vec<i64>>())
                    .unwrap_or_default()
            } else {
                Vec::new()
            }
        };
        if !payload_upgrade_ids.is_empty() {
            match vector::sync_assets_payload(&session, &payload_upgrade_ids) {
                Ok(()) => {
                    let conn = session.db.lock().unwrap();
                    let _ = conn.execute(
                        "INSERT INTO meta(key, value) VALUES('vector_payload_version', '2') \
                         ON CONFLICT(key) DO UPDATE SET value='2'",
                        [],
                    );
                }
                Err(e) => {
                    crate::logging::error(
                        "import",
                        "payload_upgrade_failed",
                        &operation,
                        &format!("assets={};message={e}", payload_upgrade_ids.len()),
                    );
                    eprintln!("Aspect payload upgrade failed; retrying next startup: {e}");
                }
            }
        }
        match crate::import::backfill_color_features(&session) {
            Ok(ids) if !ids.is_empty() => {
                eprintln!("Backfilled colors for {} assets", ids.len());
                if let Err(e) = vector::sync_assets_payload(&session, &ids) {
                    crate::logging::error(
                        "import",
                        "color_payload_sync_failed",
                        &operation,
                        &format!("assets={};message={e}", ids.len()),
                    );
                    eprintln!("Color payload synchronization failed: {e}");
                }
            }
            Ok(_) => {}
            Err(e) => {
                crate::logging::error(
                    "import",
                    "color_backfill_failed",
                    &operation,
                    &format!("message={e}"),
                );
                eprintln!("Color backfill failed; retrying next startup: {e}");
            }
        }
        let interrupted = {
            let conn = session.db.lock().unwrap();
            let result = match crate::db::assets::recoverable_imports(&conn) {
                Ok(rows) => rows,
                Err(e) => {
                    crate::logging::error(
                        "import",
                        "recoverable_scan_failed",
                        &operation,
                        &format!("message={e}"),
                    );
                    eprintln!("Recovery scan failed: {e}");
                    Vec::new()
                }
            };
            result
        };
        if !interrupted.is_empty() {
            crate::logging::info(
                "import",
                "maintenance_recovery_started",
                &operation,
                &format!("assets={}", interrupted.len()),
            );
            eprintln!(
                "Recovering {} assets with incomplete indexes",
                interrupted.len()
            );
            let mut batch = crate::import::RegisteredBatch {
                duplicates: Vec::new(),
                summary: crate::import::ImportSummary {
                    total: interrupted.len(),
                    ..Default::default()
                },
                pending: interrupted,
                aborted: false,
            };
            let cancel = app.state::<AppState>().import_ctl.cancel.clone();
            cancel.store(false, Ordering::SeqCst);
            loop {
                let more = crate::import::process_pending_chunk_with_events(
                    &app,
                    &session,
                    None,
                    &mut batch,
                    &cancel,
                    crate::import::PROCESS_CHUNK_SIZE,
                );
                // Embed each recovered chunk promptly to make assets searchable early.
                run_embedding(&app, &session, false);
                if !more {
                    break;
                }
            }
            if batch.summary.failed > 0 {
                crate::logging::warn(
                    "import",
                    "maintenance_recovery_incomplete",
                    &operation,
                    &format!(
                        "total={};done={};failed={};remaining={}",
                        batch.summary.total,
                        batch.summary.done,
                        batch.summary.failed,
                        batch.pending.len()
                    ),
                );
                eprintln!("Index recovery failed for {} assets", batch.summary.failed);
            }
        }
        // Also reconcile assets already at embedding and any missing vector points.
        run_embedding(&app, &session, true);
        if let Err(e) = vector::optimize_index(&session) {
            crate::logging::error(
                "import",
                "vector_optimize_failed",
                &operation,
                &format!("message={e}"),
            );
            eprintln!("Vector index optimization failed: {e}");
        }
        crate::logging::info(
            "import",
            "maintenance_finished",
            &operation,
            &format!(
                "elapsed_ms={};vector_ready={}",
                maintenance_started.elapsed().as_millis(),
                session.vector.lock().unwrap().is_some()
            ),
        );
        crate::indexing::spawn_worker(&app, session.clone());
        let _ = app.emit("import://maintenance-done", &session.meta.id);
    });
}
