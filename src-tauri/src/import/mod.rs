//! Two-stage import orchestration. Registration scans paths, validates duration, deduplicates
//! hashes, and records references with dimensions, emitting cards immediately alongside
//! active batches. Serialized processing probes metadata, creates covers and one whole-asset
//! index unit, then leaves embedding to vector::embed_pending. Stages: pending -> copied ->
//! probing -> thumb -> frames -> embedding -> done; failures use error:<reason>. Background
//! progress and on_asset callbacks become import://progress and import://asset events.

mod asset_events;
mod video;
use asset_events::{on_asset_and_fetch, set_stage};
use video::{import_video, VideoImportOptions};
mod preparation;
pub use preparation::register_files;
use preparation::{register_files_observed, RegistrationOptions};
#[cfg(target_os = "windows")]
mod heif_windows;
pub mod pixels;
pub(crate) mod processing_permit;
pub mod scanner;
pub mod thumb;

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter};

use crate::db;
use crate::db::model::Asset;
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession};
use crate::media::ffmpeg as ff;
use crate::util;

const REGISTER_EVENT_BATCH_SIZE: usize = 64;
/// Yield for embedding after this many assets so early imports become searchable before the
/// full batch finishes.
pub const PROCESS_CHUNK_SIZE: usize = 64;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportSummary {
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub duplicates: usize,
    /// Number of registered, duplicate, or failed source files, used only for live progress.
    #[serde(skip)]
    pub registered: usize,
}

/// No-op callbacks for tests.
pub fn noop_cbs() -> (impl Fn(&ImportSummary, &str, &str), impl Fn(&Asset)) {
    (|_, _, _| {}, |_| {})
}

pub fn emit_asset_event<R: tauri::Runtime>(app: &AppHandle<R>, library_id: &str, asset: &Asset) {
    #[derive(Clone, Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Event<'a> {
        library_id: &'a str,
        #[serde(flatten)]
        asset: &'a Asset,
    }
    let _ = app.emit("import://asset", Event { library_id, asset });
    let _ = app.emit("processing://changed", library_id);
}

/// Registration output contains pending assets and totals. Fast reference registration is
/// separate from slow processing so batches register concurrently and process serially.
pub struct RegisteredBatch {
    /// (asset_id, file_name)
    pub pending: Vec<(i64, String)>,
    pub duplicates: Vec<i64>,
    pub summary: ImportSummary,
    /// A fatal folder or directory error prevents processing this batch.
    pub aborted: bool,
}

/// Only reference imports are supported; derived files remain inside the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Copy,
    Reference,
}

impl ImportMode {
    pub fn from_setting(value: &str) -> Self {
        match value {
            "reference" => Self::Reference,
            _ => Self::Reference,
        }
    }
}

pub struct ImportBatchOptions<'a> {
    pub folder_id: Option<i64>,
    pub mode: ImportMode,
    pub policy: crate::db::indexing::IndexPolicy,
    pub batch_id: &'a str,
}

struct ProcessingInput {
    kind: scanner::Kind,
    file: PathBuf,
    imported_at: i64,
    file_name: String,
    asset_uid: String,
    visual: bool,
    index_only: bool,
}

/// Register desktop imports and emit cards per file.
pub fn register_files_with_events<R: tauri::Runtime>(
    app: &AppHandle<R>,
    session: &Arc<LibrarySession>,
    paths: &[PathBuf],
    cancel: &AtomicBool,
    options: ImportBatchOptions<'_>,
) -> RegisteredBatch {
    let ImportBatchOptions {
        folder_id,
        mode,
        policy,
        batch_id,
    } = options;

    let emitter = ImportEmitter::new(app, session, folder_id);
    let registered_paths = std::cell::RefCell::new(Vec::<String>::new());
    let emit_registered = |paths: Vec<String>| {
        if paths.is_empty() {
            return;
        }
        let _ = app.emit("processing://changed", &session.meta.id);
        let _ = app.emit(
            "import://registered-batch",
            serde_json::json!({
                "libraryId": session.meta.id,
                "sourcePaths": paths,
            }),
        );
    };
    let result = register_files_observed(
        session,
        paths,
        cancel,
        &|s, f, st| emitter.emit_progress(s, f, st),
        &|a| emitter.emit_asset(a),
        &|path| {
            let mut buffered = registered_paths.borrow_mut();
            buffered.push(path.to_string_lossy().into_owned());
            if buffered.len() >= REGISTER_EVENT_BATCH_SIZE {
                emit_registered(std::mem::take(&mut *buffered));
            }
        },
        RegistrationOptions {
            folder_id,
            mode,
            policy,
            deferred: true,
            batch_id: Some(batch_id),
        },
    );
    emit_registered(std::mem::take(&mut *registered_paths.borrow_mut()));
    result
}

/// Shared processing for imported and retried assets: probe, cover, then whole-asset index
/// preparation, with intermediate updates.
pub fn process_asset(
    session: &Arc<LibrarySession>,
    dirs: &LibraryDirs,
    asset_id: i64,
    on_asset: &dyn Fn(&Asset),
) -> AppResult<()> {
    process_asset_mode(session, dirs, asset_id, on_asset, true, false)
}

pub fn build_visual_index(session: &Arc<LibrarySession>, id: i64) -> AppResult<()> {
    process_asset_mode(
        session,
        &LibraryDirs::new(&session.root),
        id,
        &|_| {},
        true,
        true,
    )
}

fn process_asset_mode(
    session: &Arc<LibrarySession>,
    dirs: &LibraryDirs,
    asset_id: i64,
    on_asset: &dyn Fn(&Asset),
    visual: bool,
    index_only: bool,
) -> AppResult<()> {
    let _work = session.begin_asset_work(asset_id)?;
    let _asset = db::assets::get_asset(&session.read_db(), &session.root, asset_id)?
        .ok_or(AppError::AssetNotFound)?;

    // Read the asset path, kind, and storage mode.
    let (kind, abs_path, source_path, imported_at, file_name, asset_uid) = {
        let conn = session.db.lock().unwrap();
        let a = db::assets::get_asset(&conn, &session.root, asset_id)?
            .ok_or_else(|| AppError::AssetNotFound)?;
        (
            a.kind,
            a.abs_path,
            a.source_path,
            a.imported_at,
            a.file_name,
            a.uid,
        )
    };
    // Read reference media directly from its source. Mark missing sources and fail without
    // moving files or fabricating a library copy.
    if let Some(source) = source_path.as_deref() {
        let source = std::path::PathBuf::from(source);
        let missing = !source.is_file();
        {
            let conn = session.db.lock().unwrap();
            let _ = db::assets::set_source_missing(&conn, asset_id, missing);
        }
        if missing {
            let error = AppError::coded(
                "source_file_missing",
                format!("source file is missing: {source:?}"),
            );
            if !index_only {
                set_stage(session, asset_id, &format!("error:{error}"), on_asset)?;
            }
            return Err(error);
        }
        let file = source;
        let kind = match kind.as_str() {
            "image" => scanner::Kind::Image,
            "video" => scanner::Kind::Video,
            _ => scanner::Kind::Gif,
        };
        return process_file(
            session,
            dirs,
            asset_id,
            on_asset,
            ProcessingInput {
                kind,
                file,
                imported_at,
                file_name,
                asset_uid,
                visual,
                index_only,
            },
        );
    }
    // Legacy managed-file recovery reconciles interrupted staging moves and database path
    // updates before rerunning idempotent processing.
    let recorded = std::path::Path::new(&abs_path).to_path_buf();
    let final_path = dirs.asset(imported_at, &asset_uid).join(&file_name);
    let resolve_file = (|| -> AppResult<PathBuf> {
        let file = if final_path.exists() {
            final_path
        } else if recorded.exists() {
            if recorded != final_path {
                if let Some(parent) = final_path.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::rename(&recorded, &final_path).or_else(|_| {
                    fs::copy(&recorded, &final_path).and_then(|_| fs::remove_file(&recorded))
                })?;
            }
            final_path
        } else {
            return Err(AppError::coded(
                "library_file_missing",
                format!("library file missing: {abs_path}"),
            ));
        };
        if file != recorded {
            let rel = util::to_rel(&session.root, &file);
            let conn = session.db.lock().unwrap();
            db::assets::set_asset_rel_path(&conn, asset_id, &rel, &file_name)?;
        }
        Ok(file)
    })();
    let file = match resolve_file {
        Ok(file) => file,
        Err(error) => {
            if !index_only {
                set_stage(session, asset_id, &format!("error:{error}"), on_asset)?;
            }
            return Err(error);
        }
    };
    let kind = match kind.as_str() {
        "image" => scanner::Kind::Image,
        "video" => scanner::Kind::Video,
        _ => scanner::Kind::Gif,
    };
    process_file(
        session,
        dirs,
        asset_id,
        on_asset,
        ProcessingInput {
            kind,
            file,
            imported_at,
            file_name,
            asset_uid,
            visual,
            index_only,
        },
    )
}

/// Shared cover and index-input processing after resolving the source path.
fn process_file(
    session: &Arc<LibrarySession>,
    dirs: &LibraryDirs,
    asset_id: i64,
    on_asset: &dyn Fn(&Asset),
    input: ProcessingInput,
) -> AppResult<()> {
    let ProcessingInput {
        kind,
        file,
        imported_at,
        file_name,
        asset_uid,
        visual,
        index_only,
    } = input;

    let result = (|| -> AppResult<()> {
        // Keep a published cover outside the keyframe directory before rebuilding it.
        if matches!(kind, scanner::Kind::Video) {
            let conn = session.db.lock().unwrap();
            let manual = db::assets::has_manual_cover(&conn, asset_id)?;
            let asset = db::assets::get_asset(&conn, &session.root, asset_id)?
                .ok_or(AppError::AssetNotFound)?;
            if let Some(source) = asset.thumb_path {
                if manual
                    || std::path::Path::new(&source)
                        .starts_with(dirs.asset_keyframes(imported_at, &asset_uid))
                {
                    let cover = dirs.thumbnail(imported_at, &asset_uid, &file_name);
                    let preserved = if manual {
                        cover.with_extension("manual.webp")
                    } else {
                        cover
                    };
                    if std::path::Path::new(&source) != preserved {
                        util::replace_file(&preserved, |temporary| {
                            fs::copy(&source, temporary)?;
                            Ok(())
                        })?;
                        conn.execute(
                            "UPDATE assets SET thumb_rel=?1 WHERE id=?2",
                            rusqlite::params![util::to_rel(&session.root, &preserved), asset_id],
                        )?;
                    }
                }
            }
        }
        {
            let conn = session.db.lock().unwrap();
            conn.execute("UPDATE assets SET visual_indexed=0 WHERE id=?", [asset_id])?;
            if !visual && crate::db::indexing::policy(&conn, asset_id)?.visual {
                crate::db::indexing::enqueue(&conn, asset_id, "visual")?;
            }
        }
        // Remove previous derived inputs and vector points by asset ID to make retries
        // idempotent, including points whose SQLite rows are already gone.
        crate::vector::delete_asset_points(session, asset_id)?;
        {
            let conn = session.db.lock().unwrap();
            conn.execute(
                "DELETE FROM frames WHERE asset_id = ?1",
                rusqlite::params![asset_id],
            )?;
        }
        let _ = fs::remove_dir_all(dirs.asset_keyframes(imported_at, &asset_uid));

        if !index_only {
            set_stage(session, asset_id, "probing", on_asset)?;
        }
        match kind {
            scanner::Kind::Image => {
                if !index_only {
                    set_stage(session, asset_id, "thumb", on_asset)?;
                }
                import_image(session, dirs, asset_id, &file, visual)?;
            }
            // Probe videos and GIFs before preparing their covers and index inputs.
            scanner::Kind::Video | scanner::Kind::Gif => {
                import_video(
                    session,
                    dirs,
                    asset_id,
                    &file,
                    on_asset,
                    VideoImportOptions {
                        is_gif: kind == scanner::Kind::Gif,
                        visual,
                        index_only,
                    },
                )?;
            }
        }
        if index_only {
            Ok(())
        } else {
            set_stage(
                session,
                asset_id,
                if visual { "embedding" } else { "done" },
                on_asset,
            )
        }
    })();

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            if !index_only {
                set_stage(session, asset_id, &format!("error:{e}"), on_asset)?;
            }
            Err(e)
        }
    }
}

/// Import controls stored in AppState.
#[derive(Default)]
pub struct ImportControl {
    /// Whether import work is active; reset when the worker exits.
    pub running: Arc<AtomicBool>,
    /// Cancellation flag.
    pub cancel: Arc<AtomicBool>,
}

#[cfg(test)]
#[path = "../../tests/unit/import_recovery.rs"]
mod recovery_tests;

mod batches;
mod events;
mod images;

pub use batches::*;
pub use events::*;
pub use images::backfill_color_features;
use images::{import_image, thumbnail_path};
