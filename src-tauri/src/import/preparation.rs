//! File registration, duplicate recovery and reference relinking.

use super::{scanner, set_stage, thumb, ImportMode, ImportSummary, RegisteredBatch};
use crate::db::model::Asset;
use crate::{
    db,
    error::{AppError, AppResult},
    library::{LibraryDirs, LibrarySession},
    media::ffmpeg as ff,
    util,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::{fs, path::PathBuf, sync::Arc};

enum PrepareOutcome {
    Pending(i64),
    Restored(i64),
    Duplicate(i64),
}

/// Current or modification time in Unix seconds.
fn unix_seconds(system_time: std::io::Result<std::time::SystemTime>, fallback: i64) -> i64 {
    system_time
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(fallback)
}

/// Complete import processing for one file.
fn prepare_one(
    session: &Arc<LibrarySession>,
    src: &std::path::Path,
    _dest_dir: &std::path::Path,
    folder_id: Option<i64>,
    mode: ImportMode,
    on_asset: &dyn Fn(&Asset),
) -> AppResult<PrepareOutcome> {
    if mode != ImportMode::Reference {
        return Err(AppError::InvalidArgument(
            "Only reference imports are supported".into(),
        ));
    }
    let mut work = Some(session.asset_activity.work()?);
    let kind = scanner::classify(src).ok_or_else(|| {
        AppError::coded(
            "unsupported_file_type",
            format!("unsupported file type: {}", src.display()),
        )
    })?;

    // Deduplicate by hash; restore matching trashed assets into the requested folder.
    if matches!(scanner::classify(src), Some(scanner::Kind::Video)) {
        let info = crate::embedding::media::Media {
            ffmpeg: crate::media::ffmpeg_path(),
            ffprobe: crate::model_download::runtime().join(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            }),
        }
        .video_info(src)
        .map_err(|e| AppError::InvalidArgument(e.to_string()))?;
        let _ = info;
    }
    let hash = scanner::file_hash(src)?;
    let duplicate_id = {
        let conn = session.db.lock().unwrap();
        db::assets::find_by_hash(&conn, &hash)?
    };
    if let Some(id) = duplicate_id {
        let existing = {
            let conn = session.db.lock().unwrap();
            db::assets::get_asset(&conn, &session.root, id)?
        };
        let deleted = existing
            .as_ref()
            .and_then(|asset| asset.deleted_at)
            .is_some();

        // Relink a missing reference to the reimported path so deduplication cannot prevent
        // recovery of moved sources.
        if mode == ImportMode::Reference
            && existing.as_ref().is_some_and(|a| {
                a.storage == "reference"
                    && (a.source_missing || !std::path::Path::new(&a.abs_path).is_file())
            })
        {
            let mtime = unix_seconds(fs::metadata(src).and_then(|m| m.modified()), util::now());
            {
                let conn = session.db.lock().unwrap();
                db::assets::relink_reference(&conn, id, &src.to_string_lossy(), mtime)?;
            }
            crate::services::assets::move_assets_to_folder(session, &[id], folder_id)?;
            let asset = db::assets::get_asset(&session.read_db(), &session.root, id)?;
            if let Some(asset) = asset {
                on_asset(&asset);
            }
            return Ok(PrepareOutcome::Restored(id));
        }

        if deleted {
            drop(work.take());
            crate::services::assets::restore_assets(session, &[id])?;
            let _restored_work = session.asset_activity.work()?;
            crate::services::assets::move_assets_to_folder(session, &[id], folder_id)?;
            let asset = db::assets::get_asset(&session.read_db(), &session.root, id)?;
            if let Some(asset) = asset {
                on_asset(&asset);
            }
            return Ok(PrepareOutcome::Restored(id));
        }
        return Ok(PrepareOutcome::Duplicate(id));
    }

    // Register the source and obtain its asset ID.
    let file_name = src
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let imported_at = util::now();
    let asset_uid = util::new_entity_uid();

    // Reference mode records the original path without copying it; rel_path reserves a
    // location for derived previews and index inputs.
    if mode == ImportMode::Reference {
        let meta = fs::metadata(src)?;
        let file_size = meta.len() as i64;
        let source_mtime = unix_seconds(meta.modified(), imported_at);
        let asset_dir = LibraryDirs::new(&session.root).asset(imported_at, &asset_uid);
        let rel = util::to_rel(&session.root, &asset_dir.join(&file_name));
        let asset_id = {
            let conn = session.db.lock().unwrap();
            db::assets::insert_asset(
                &conn,
                crate::db::assets::NewAsset {
                    folder_id,
                    file_name: &file_name,
                    rel_path: &rel,
                    file_hash: &hash,
                    file_size,
                    mime_type: &scanner::mime_of(src),
                    kind: kind.as_str(),
                    imported_at,
                    uid: Some(&asset_uid),
                    source: Some(crate::db::assets::ReferenceSource {
                        path: &src.to_string_lossy(),
                        modified_at: source_mtime,
                    }),
                },
            )?
        };
        let _work = session.begin_asset_work(asset_id)?;
        if let Err(e) = fs::create_dir_all(&asset_dir) {
            let conn = session.db.lock().unwrap();
            let _ = db::assets::delete_assets(&conn, &[asset_id]);
            return Err(e.into());
        }
        if let Err(e) = register_probe_meta(session, &kind, src, asset_id) {
            crate::logging::warn("import", "probe_meta_failed", "-", &format!("message={e}"));
        }
        set_stage(session, asset_id, "copied", on_asset)?;
        return Ok(PrepareOutcome::Pending(asset_id));
    }

    Err(AppError::InvalidArgument(
        "Only reference imports are supported".into(),
    ))
}

/// Best-effort dimensions and duration let cards reserve the final aspect ratio immediately.
/// Processing probes again and reports any failure.
fn register_probe_meta(
    session: &LibrarySession,
    kind: &scanner::Kind,
    file: &std::path::Path,
    asset_id: i64,
) -> AppResult<()> {
    let (width, height, duration_ms) = match kind {
        scanner::Kind::Image => {
            let (w, h) = thumb::image_dimensions(file)?;
            (Some(w as i64), Some(h as i64), None)
        }
        scanner::Kind::Video | scanner::Kind::Gif => {
            let info = ff::probe(file)?;
            (info.width, info.height, info.duration_ms)
        }
    };
    let conn = session.db.lock().unwrap();
    db::assets::update_asset_probe(
        &conn,
        asset_id,
        crate::db::assets::AssetProbe {
            width,
            height,
            duration_ms,
            color: None,
            thumb_rel: None,
        },
    )
}

/// Scan and deduplicate paths, registering references and emitting cards immediately instead
/// of waiting for the entire batch.
pub fn register_files(
    session: &Arc<LibrarySession>,
    paths: &[PathBuf],
    folder_id: Option<i64>,
    cancel: &AtomicBool,
    mode: ImportMode,
    progress: &dyn Fn(&ImportSummary, &str, &str),
    on_asset: &dyn Fn(&Asset),
) -> RegisteredBatch {
    register_files_observed(
        session,
        paths,
        cancel,
        progress,
        on_asset,
        &|_| {},
        RegistrationOptions {
            folder_id,
            mode,
            policy: crate::db::indexing::IndexPolicy::default(),
            deferred: false,
            batch_id: None,
        },
    )
}

pub(super) struct RegistrationOptions<'a> {
    pub folder_id: Option<i64>,
    pub mode: ImportMode,
    pub policy: crate::db::indexing::IndexPolicy,
    pub deferred: bool,
    pub batch_id: Option<&'a str>,
}

pub(super) fn register_files_observed(
    session: &Arc<LibrarySession>,
    paths: &[PathBuf],
    cancel: &AtomicBool,
    progress: &dyn Fn(&ImportSummary, &str, &str),
    on_asset: &dyn Fn(&Asset),
    on_registered: &dyn Fn(&std::path::Path),
    options: RegistrationOptions<'_>,
) -> RegisteredBatch {
    let RegistrationOptions {
        folder_id,
        mode,
        policy,
        deferred,
        batch_id,
    } = options;
    let mut summary = ImportSummary::default();
    let dirs = LibraryDirs::new(&session.root);
    let owned_batch = util::new_id();
    let batch_id = batch_id.unwrap_or(&owned_batch);
    let (files, issues) = scanner::scan_report(paths);
    summary.total = files.len() + issues.len();
    summary.failed = issues.len();
    let records = (|| -> AppResult<Vec<i64>> {
        let mut conn = session.db.lock().unwrap();
        let tx = conn.transaction()?;
        let ids = files
            .iter()
            .map(|path| db::processing::register(&tx, batch_id, path, folder_id))
            .collect::<AppResult<Vec<_>>>()?;
        for (path, error) in &issues {
            let id = db::processing::register(&tx, batch_id, path, folder_id)?;
            db::processing::registered(&tx, id, None, "failed", Some(error))?;
        }
        tx.commit()?;
        Ok(ids)
    })();
    let records = match records {
        Ok(ids) => ids,
        Err(error) => {
            crate::logging::error("import", "queue_record_failed", "-", &error.to_string());
            return RegisteredBatch {
                pending: Vec::new(),
                duplicates: Vec::new(),
                summary,
                aborted: true,
            };
        }
    };

    // Folder membership is logical; derived data uses per-asset directories.
    let dest_dir = match folder_id {
        Some(fid) => {
            let conn = session.db.lock().unwrap();
            match db::folders::get_name(&conn, fid) {
                Ok(Some(_)) => dirs.app().join("importing"),
                _ => {
                    let _ = db::processing::finish_batch(
                        &conn,
                        batch_id,
                        "failed",
                        Some("target folder does not exist"),
                    );
                    progress(&summary, "target folder does not exist", "error");
                    return RegisteredBatch {
                        pending: Vec::new(),
                        duplicates: Vec::new(),
                        summary,
                        aborted: true,
                    };
                }
            }
        }
        None => dirs.app().join("importing"),
    };
    let create_staging = session
        .asset_activity
        .work()
        .and_then(|_work| fs::create_dir_all(&dest_dir).map_err(AppError::from));
    if let Err(e) = create_staging {
        let _ = db::processing::finish_batch(
            &session.db.lock().unwrap(),
            batch_id,
            "failed",
            Some(&e.to_string()),
        );
        progress(
            &summary,
            &format!("failed to create target directory: {e}"),
            "error",
        );
        return RegisteredBatch {
            pending: Vec::new(),
            duplicates: Vec::new(),
            summary,
            aborted: true,
        };
    }

    // Scan inputs.
    let mut pending = Vec::new();
    let mut duplicates = Vec::new();
    for (path, record) in files.iter().zip(records) {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        progress(&summary, &file_name, "copy");
        let _ =
            db::processing::registered(&session.db.lock().unwrap(), record, None, "running", None);

        match prepare_one(session, path, &dest_dir, folder_id, mode, on_asset) {
            Ok(PrepareOutcome::Pending(asset_id)) => {
                {
                    let conn = session.db.lock().unwrap();
                    let _ =
                        db::processing::registered(&conn, record, Some(asset_id), "queued", None);
                    let _ = crate::db::indexing::set_policy(&conn, asset_id, policy);
                    if policy.visual && deferred {
                        let _ = crate::db::indexing::enqueue(&conn, asset_id, "visual");
                    }
                }

                pending.push((asset_id, file_name.clone()));
            }
            Ok(PrepareOutcome::Restored(asset_id)) => {
                let conn = session.db.lock().unwrap();
                let state = db::assets::get_asset(&conn, &session.root, asset_id)
                    .ok()
                    .flatten()
                    .map(|a| {
                        if matches!(a.import_stage.as_str(), "done" | "embedding") {
                            "completed"
                        } else {
                            "queued"
                        }
                    })
                    .unwrap_or("queued");
                let _ = db::processing::registered(&conn, record, Some(asset_id), state, None);
                drop(conn);
                pending.push((asset_id, file_name.clone()));
                summary.done += 1;
            }
            Ok(PrepareOutcome::Duplicate(id)) => {
                let conn = session.db.lock().unwrap();
                let existing = db::assets::get_asset(&conn, &session.root, id)
                    .ok()
                    .flatten();
                let failure = existing
                    .as_ref()
                    .and_then(|a| a.import_stage.strip_prefix("error:"));
                let _ = db::processing::registered(
                    &conn,
                    record,
                    Some(id),
                    if failure.is_some() {
                        "failed"
                    } else {
                        "skipped"
                    },
                    failure,
                );
                drop(conn);
                summary.duplicates += 1;
                duplicates.push(id);
            }
            Err(e) => {
                summary.failed += 1;
                let _ = db::processing::registered(
                    &session.db.lock().unwrap(),
                    record,
                    None,
                    "failed",
                    Some(&e.to_string()),
                );
                {
                    let _conn = session.db.lock().unwrap();
                }
                crate::logging::error(
                    "import",
                    "asset_failed",
                    "-",
                    &format!("file=<redacted>;message={e};name_len={}", file_name.len()),
                );
            }
        }
        summary.registered += 1;
        // Advance registration progress per file so large batches do not misleadingly remain
        // at zero until processing starts.
        progress(&summary, &file_name, "copy");
        on_registered(path);
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = db::processing::finish_batch(
            &session.db.lock().unwrap(),
            batch_id,
            "interrupted",
            Some("Import cancelled before processing completed"),
        );
    }
    RegisteredBatch {
        pending,
        duplicates,
        summary,
        aborted: cancel.load(Ordering::Relaxed),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/import_preparation.rs"]
mod tests;
