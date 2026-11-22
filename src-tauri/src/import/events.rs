//! Desktop progress and asset events for batch processing and embedding.

use super::batches::process_pending_chunk_inner;
use super::{emit_asset_event, ImportSummary, RegisteredBatch};
use crate::db::model::Asset;
use crate::library::LibrarySession;
use serde::Serialize;
use std::sync::{atomic::AtomicBool, Arc};
use tauri::{AppHandle, Emitter};

/// Progress event payload matching the frontend ImportProgress type.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    library_id: String,
    folder_id: Option<i64>,
    total: usize,
    done: usize,
    current_file: String,
    stage: &'static str,
    failed: usize,
}

/// Forward batch progress and asset-state changes to desktop events.
pub struct ImportEmitter<R: tauri::Runtime = tauri::Wry> {
    app: AppHandle<R>,
    library_id: String,
    folder_id: Option<i64>,
}

impl<R: tauri::Runtime> ImportEmitter<R> {
    pub fn new(app: &AppHandle<R>, session: &LibrarySession, folder_id: Option<i64>) -> Self {
        Self {
            app: app.clone(),
            library_id: session.meta.id.clone(),
            folder_id,
        }
    }

    pub(super) fn emit_progress(&self, s: &ImportSummary, file: &str, stage: &str) {
        let _ = self.app.emit(
            "import://progress",
            Progress {
                library_id: self.library_id.clone(),
                folder_id: self.folder_id,
                total: s.total,
                done: if stage == "copy" {
                    s.registered
                } else {
                    s.done + s.duplicates + s.failed
                },
                current_file: file.to_string(),
                stage: stage_static(stage),
                failed: s.failed,
            },
        );
    }

    pub(super) fn emit_asset(&self, a: &Asset) {
        emit_asset_event(&self.app, &self.library_id, a);
    }
}

/// Process media and emit intermediate state changes.
pub fn process_pending_with_events<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<LibrarySession>,
    folder_id: Option<i64>,
    batch: &mut RegisteredBatch,
    cancel: &AtomicBool,
) {
    process_pending_chunk_with_events(app, session, folder_id, batch, cancel, usize::MAX);
}

/// Process one chunk and report whether more assets remain.
pub fn process_pending_chunk_with_events<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<LibrarySession>,
    folder_id: Option<i64>,
    batch: &mut RegisteredBatch,
    cancel: &AtomicBool,
    limit: usize,
) -> bool {
    let emitter = ImportEmitter::new(app, session, folder_id);
    process_pending_chunk_inner(
        session,
        batch,
        cancel,
        limit,
        &|s, f, st| emitter.emit_progress(s, f, st),
        &|a| emitter.emit_asset(a),
        false,
    )
}

fn stage_static(s: &str) -> &'static str {
    match s {
        "copy" => "copy",
        "probe" => "probe",
        "thumb" => "thumb",
        "frames" => "frames",
        "embed" => "embed",
        "finish" => "finish",
        _ => "error",
    }
}

/// Emit embedding progress using the import event shape; total/done count index units and
/// current_file identifies the asset.
pub fn emit_embed_progress<R: tauri::Runtime>(
    app: &AppHandle<R>,
    library_id: &str,
    total: usize,
    done: usize,
    file: &str,
) {
    let _ = app.emit(
        "import://progress",
        Progress {
            library_id: library_id.to_string(),
            folder_id: None,
            total,
            done,
            current_file: file.to_string(),
            stage: "embed",
            failed: 0,
        },
    );
}
