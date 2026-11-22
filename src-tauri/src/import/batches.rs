//! Batch scheduling, policy-aware imports and retry iteration.

use super::{
    process_asset_mode, register_files, register_files_observed, ImportMode, ImportSummary,
    RegisteredBatch, RegistrationOptions, PROCESS_CHUNK_SIZE,
};
use crate::library::{LibraryDirs, LibrarySession};
use crate::{db, db::model::Asset};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

/// Process each asset through probing, thumbnail generation, and index-input preparation,
/// stopping before embedding.
pub fn process_pending(
    session: &Arc<LibrarySession>,
    batch: &mut RegisteredBatch,
    cancel: &AtomicBool,
    progress: &dyn Fn(&ImportSummary, &str, &str),
    on_asset: &dyn Fn(&Asset),
) {
    while process_pending_chunk(session, batch, cancel, usize::MAX, progress, on_asset) {}
}

/// Process at most limit assets and report whether work remains.
pub fn process_pending_chunk(
    session: &Arc<LibrarySession>,
    batch: &mut RegisteredBatch,
    cancel: &AtomicBool,
    limit: usize,
    progress: &dyn Fn(&ImportSummary, &str, &str),
    on_asset: &dyn Fn(&Asset),
) -> bool {
    process_pending_chunk_inner(session, batch, cancel, limit, progress, on_asset, true)
}

pub(super) fn process_pending_chunk_inner(
    session: &Arc<LibrarySession>,
    batch: &mut RegisteredBatch,
    cancel: &AtomicBool,
    limit: usize,
    progress: &dyn Fn(&ImportSummary, &str, &str),
    on_asset: &dyn Fn(&Asset),
    visual: bool,
) -> bool {
    let dirs = LibraryDirs::new(&session.root);
    let take = limit.min(batch.pending.len());
    let current: Vec<_> = batch.pending.drain(..take).collect();
    for (asset_id, file_name) in current {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        progress(&batch.summary, &file_name, "probe");
        // Recovered assets at done or embedding already have derived inputs; retry only
        // intermediate or failed media processing.
        let already_done = {
            let conn = session.db.lock().unwrap();
            db::assets::get_asset(&conn, &session.root, asset_id)
                .ok()
                .flatten()
                .is_some_and(|asset| {
                    asset.import_stage == "done" || asset.import_stage == "embedding"
                })
        };
        if already_done {
            continue;
        }
        match process_asset_mode(session, &dirs, asset_id, on_asset, visual, false) {
            Ok(()) => batch.summary.done += 1,
            Err(e) => {
                batch.summary.failed += 1;
                {
                    let _conn = session.db.lock().unwrap();
                }
                crate::logging::error(
                    "import",
                    "processing_failed",
                    "-",
                    &format!("file=<redacted>;message={e};name_len={}", file_name.len()),
                );
            }
        }
    }
    !batch.pending.is_empty() && !cancel.load(Ordering::Relaxed)
}

/// Register and process an import, stopping promptly on cancellation while preserving
/// completed files.
pub fn run_import(
    session: &Arc<LibrarySession>,
    paths: &[PathBuf],
    folder_id: Option<i64>,
    cancel: &AtomicBool,
    mode: ImportMode,
    progress: &dyn Fn(&ImportSummary, &str, &str),
    on_asset: &dyn Fn(&Asset),
) -> ImportSummary {
    let mut batch = register_files(session, paths, folder_id, cancel, mode, progress, on_asset);
    if !batch.aborted {
        process_pending(session, &mut batch, cancel, progress, on_asset);
        progress(&batch.summary, "", "finish");
    }
    batch.summary
}

/// Observers shared by registration and processing for a single import.
pub struct ImportCallbacks<'a> {
    pub progress: &'a dyn Fn(&ImportSummary, &str, &str),
    pub on_asset: &'a dyn Fn(&Asset),
}

pub fn run_import_with_policy(
    session: &Arc<LibrarySession>,
    paths: &[PathBuf],
    folder_id: Option<i64>,
    cancel: &AtomicBool,
    mode: ImportMode,
    policy: crate::db::indexing::IndexPolicy,
    callbacks: ImportCallbacks<'_>,
) -> ImportSummary {
    let mut batch = register_files_observed(
        session,
        paths,
        cancel,
        callbacks.progress,
        callbacks.on_asset,
        &|_| {},
        RegistrationOptions {
            folder_id,
            mode,
            policy,
            deferred: true,
            batch_id: None,
        },
    );
    while !batch.aborted
        && process_pending_chunk_inner(
            session,
            &mut batch,
            cancel,
            PROCESS_CHUNK_SIZE,
            callbacks.progress,
            callbacks.on_asset,
            false,
        )
    {}
    batch.summary
}

/// Reprocess failed or interrupted assets by ID.
pub fn reprocess_assets(
    session: &Arc<LibrarySession>,
    ids: &[i64],
    on_asset: &dyn Fn(&Asset),
) -> (usize, usize) {
    reprocess_assets_cancellable(session, ids, on_asset, &AtomicBool::new(false))
}

pub fn reprocess_assets_cancellable(
    session: &Arc<LibrarySession>,
    ids: &[i64],
    on_asset: &dyn Fn(&Asset),
    cancel: &AtomicBool,
) -> (usize, usize) {
    reprocess_mode(session, ids, on_asset, cancel, true)
}
pub fn reprocess_assets_base_cancellable(
    session: &Arc<LibrarySession>,
    ids: &[i64],
    on_asset: &dyn Fn(&Asset),
    cancel: &AtomicBool,
) -> (usize, usize) {
    reprocess_mode(session, ids, on_asset, cancel, false)
}
fn reprocess_mode(
    session: &Arc<LibrarySession>,
    ids: &[i64],
    on_asset: &dyn Fn(&Asset),
    cancel: &AtomicBool,
    visual: bool,
) -> (usize, usize) {
    let dirs = LibraryDirs::new(&session.root);
    let mut ok = 0;
    let mut failed = 0;
    for id in ids {
        if cancel.load(Ordering::SeqCst) {
            break;
        }
        if db::processing::resume_or_retry_import(&session.db.lock().unwrap(), *id).is_err() {
            failed += 1;
            continue;
        }
        match process_asset_mode(session, &dirs, *id, on_asset, visual, false) {
            Ok(()) => ok += 1,
            Err(_) => failed += 1,
        }
    }
    (ok, failed)
}
