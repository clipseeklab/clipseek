use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;

use qdrant_edge::{PointId, RetrieveRequest};
use serde::{Deserialize, Serialize};

use crate::clip::ClipEncoder;
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession, VectorHealth};

use super::{frame_payload_json, open_index, upsert_points_to_shard};

const REBUILD_BATCH: i64 = 64;
const JOURNAL_FILE: &str = "vector-rebuild.json";
const ESTIMATED_BYTES_PER_IMAGE: u64 = 3_072;
const REBUILD_MIN_HEADROOM: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RebuildPhase {
    Building,
    Verified,
    OldIndexMoved,
    NewIndexMoved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildJournal {
    pub rebuild_id: String,
    pub phase: RebuildPhase,
    pub staging_dir: String,
    pub backup_dir: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryOutcome {
    pub restored_backup: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VectorStatus {
    pub library_id: String,
    pub state: String,
    pub code: Option<String>,
    pub message: Option<String>,
    pub rebuild_id: Option<String>,
    pub failed_frames: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildPreflight {
    pub library_id: String,
    pub frame_count: i64,
    pub current_index_bytes: u64,
    pub estimated_required_bytes: u64,
    pub available_bytes: u64,
    pub can_start: bool,
    pub reason_code: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildProgress {
    pub library_id: String,
    pub rebuild_id: String,
    pub phase: String,
    pub completed: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RebuildReport {
    pub indexed_frames: usize,
    pub failed_frames: usize,
}

fn journal_path(root: &Path) -> PathBuf {
    LibraryDirs::new(root).app().join(JOURNAL_FILE)
}

fn write_journal(root: &Path, journal: &RebuildJournal) -> AppResult<()> {
    let path = journal_path(root);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(journal)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

fn file_disk_bytes(metadata: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        // st_blocks reports allocated 512-byte blocks and does not count sparse-file holes.
        metadata.blocks().saturating_mul(512)
    }
    #[cfg(not(unix))]
    {
        metadata.len()
    }
}

fn directory_disk_bytes(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| {
            entry
                .metadata()
                .map(|meta| {
                    if meta.is_dir() {
                        directory_disk_bytes(&entry.path())
                    } else {
                        file_disk_bytes(&meta)
                    }
                })
                .unwrap_or(0)
        })
        .sum()
}

fn estimated_rebuild_free_space(current_index_bytes: u64, image_count: i64) -> u64 {
    let estimated_new_index = (image_count.max(0) as u64)
        .saturating_mul(ESTIMATED_BYTES_PER_IMAGE)
        .max(current_index_bytes);
    // available_space() has already excluded the old index. Reserve up to two copies of the
    // staging index for segment optimization, plus fixed headroom for journals and metadata.
    estimated_new_index
        .saturating_mul(2)
        .saturating_add(REBUILD_MIN_HEADROOM)
}

/// Startup repairs directory-swap consistency only; it does not encode or start a rebuild.
pub fn recover_interrupted_swap(root: &Path) -> AppResult<RecoveryOutcome> {
    let path = journal_path(root);
    if !path.is_file() {
        return Ok(RecoveryOutcome {
            restored_backup: false,
        });
    }
    let journal: RebuildJournal = serde_json::from_slice(&fs::read(&path)?).map_err(|error| {
        AppError::coded(
            "vector_rebuild_journal_invalid",
            format!("invalid rebuild journal: {error}"),
        )
    })?;
    let valid_name = |name: &str, prefix: &str| {
        name.starts_with(prefix)
            && Path::new(name).components().count() == 1
            && !name.contains(['/', '\\'])
    };
    if !valid_name(&journal.staging_dir, "vector.rebuilding-")
        || !valid_name(&journal.backup_dir, "vector.backup-")
    {
        return Err(AppError::coded(
            "vector_rebuild_journal_invalid",
            "rebuild journal contains an invalid directory name",
        ));
    }
    let app = LibraryDirs::new(root).app();
    let current = app.join("vector");
    let backup = app.join(&journal.backup_dir);
    let mut restored_backup = false;
    if journal.phase == RebuildPhase::OldIndexMoved && current.exists() && backup.exists() {
        // A directory rename may have committed before its journal update. Restore the old
        // index rather than trusting an unrecorded swap.
        let staging = app.join(&journal.staging_dir);
        if !staging.exists() {
            fs::rename(&current, &staging)?;
        }
        fs::rename(&backup, &current)?;
        restored_backup = true;
    } else if !current.exists() && backup.exists() {
        fs::rename(&backup, &current)?;
        restored_backup = true;
    }
    // The journal describes directory swaps only. Remove it after rollback and leave staging
    // for the next user-confirmed rebuild.
    if restored_backup || current.exists() {
        let _ = fs::remove_file(&path);
    }
    Ok(RecoveryOutcome { restored_backup })
}

pub fn status(session: &LibrarySession) -> VectorStatus {
    let health = session.vector_health.lock().unwrap().clone();
    match health {
        VectorHealth::Ready => VectorStatus {
            library_id: session.meta.id.clone(),
            state: "ready".into(),
            code: None,
            message: None,
            rebuild_id: None,
            failed_frames: None,
        },
        VectorHealth::RebuildPending { code, message } => VectorStatus {
            library_id: session.meta.id.clone(),
            state: "rebuild_pending".into(),
            code: Some(code),
            message: Some(message),
            rebuild_id: None,
            failed_frames: None,
        },
        VectorHealth::Rebuilding { rebuild_id } => VectorStatus {
            library_id: session.meta.id.clone(),
            state: "rebuilding".into(),
            code: None,
            message: None,
            rebuild_id: Some(rebuild_id),
            failed_frames: None,
        },
    }
}

pub fn retry_open(session: &LibrarySession) -> AppResult<VectorStatus> {
    if session.vector.lock().unwrap().is_some() {
        *session.vector_health.lock().unwrap() = VectorHealth::Ready;
        return Ok(status(session));
    }
    recover_interrupted_swap(&session.root)?;
    let operation = session.meta.id.chars().take(8).collect::<String>();
    let started = std::time::Instant::now();
    crate::logging::info(
        "vector",
        "retry_open_started",
        &operation,
        &super::index_storage_summary(&LibraryDirs::new(&session.root).vector()),
    );
    match open_index(&LibraryDirs::new(&session.root).vector()) {
        Ok(shard) => {
            *session.vector.lock().unwrap() = Some(shard);
            *session.vector_health.lock().unwrap() = VectorHealth::Ready;
            crate::logging::info(
                "vector",
                "retry_open_succeeded",
                &operation,
                &format!("elapsed_ms={}", started.elapsed().as_millis()),
            );
            Ok(status(session))
        }
        Err(error) => {
            let message = error.to_string();
            *session.vector_health.lock().unwrap() = VectorHealth::RebuildPending {
                code: "vector_index_open_failed".into(),
                message: message.clone(),
            };
            crate::logging::error(
                "vector",
                "retry_open_failed",
                &operation,
                &format!(
                    "elapsed_ms={};message={message};{}",
                    started.elapsed().as_millis(),
                    super::index_storage_summary(&LibraryDirs::new(&session.root).vector())
                ),
            );
            Err(AppError::coded("vector_index_open_failed", message))
        }
    }
}

pub fn preflight(session: &LibrarySession) -> AppResult<RebuildPreflight> {
    // Preflight counts use a dedicated read connection so startup maintenance holding the
    // writer cannot block the rebuild-information dialog.
    let frame_count = session
        .read_db()
        .query_row("SELECT COUNT(*) FROM frames fr JOIN assets a ON a.id=fr.asset_id WHERE a.visual_indexed=1 OR (a.visual_requested=1 AND NOT EXISTS(SELECT 1 FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual'))", [], |row| {
            row.get::<_, i64>(0)
        })?;
    if frame_count == 0 {
        let asset_count: i64 = session.read_db().query_row(
            "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL",
            [],
            |row| row.get(0),
        )?;
        if asset_count > 0 {
            return Err(AppError::coded(
                "vector_rebuild_no_frames",
                format!(
                    "{asset_count} assets have no indexable frames; retry failed imports before rebuilding"
                ),
            ));
        }
    }
    let vector_dir = LibraryDirs::new(&session.root).vector();
    // Sparse preallocation can make logical vector-file lengths much larger than actual disk
    // usage.
    let current_index_bytes = directory_disk_bytes(&vector_dir);
    let estimated_required_bytes = estimated_rebuild_free_space(current_index_bytes, frame_count);
    let available_bytes = fs2::available_space(LibraryDirs::new(&session.root).app()).unwrap_or(0);
    let can_start = available_bytes >= estimated_required_bytes;
    Ok(RebuildPreflight {
        library_id: session.meta.id.clone(),
        frame_count,
        current_index_bytes,
        estimated_required_bytes,
        available_bytes,
        can_start,
        reason_code: (!can_start).then(|| "vector_rebuild_insufficient_space".into()),
    })
}

pub fn rebuild(
    session: &Arc<LibrarySession>,
    clip: &ClipEncoder,
    rebuild_id: &str,
    on_progress: &dyn Fn(RebuildProgress),
) -> AppResult<RebuildReport> {
    rebuild_with_encoder(session, rebuild_id, on_progress, &|images| {
        clip.encode_images(images)
    })
}

type ImageEncoder<'a> = dyn Fn(&[image::DynamicImage]) -> AppResult<Vec<Vec<f32>>> + 'a;

fn rebuild_with_encoder(
    session: &Arc<LibrarySession>,
    rebuild_id: &str,
    on_progress: &dyn Fn(RebuildProgress),
    encode: &ImageEncoder<'_>,
) -> AppResult<RebuildReport> {
    let _activity = session.asset_activity.cleanup()?;
    let preflight = preflight(session)?;
    if !preflight.can_start {
        return Err(AppError::coded(
            "vector_rebuild_insufficient_space",
            "not enough free space to rebuild vector index",
        ));
    }
    session.vector_rebuild_cancel.store(false, Ordering::SeqCst);
    let dirs = LibraryDirs::new(&session.root);
    let staging_name = format!("vector.rebuilding-{rebuild_id}");
    let backup_name = format!("vector.backup-{rebuild_id}");
    let staging = dirs.app().join(&staging_name);
    let backup = dirs.app().join(&backup_name);
    // Only a newly confirmed task cleans failed staging; startup recovery must preserve it.
    if let Ok(entries) = fs::read_dir(dirs.app()) {
        for entry in entries.flatten() {
            if entry
                .file_name()
                .to_string_lossy()
                .starts_with("vector.rebuilding-")
            {
                fs::remove_dir_all(entry.path())?;
            }
        }
    }
    let mut journal = RebuildJournal {
        rebuild_id: rebuild_id.into(),
        phase: RebuildPhase::Building,
        staging_dir: staging_name,
        backup_dir: backup_name,
        updated_at: crate::util::now(),
    };
    write_journal(&session.root, &journal)?;
    let shard = open_index(&staging)?;
    let total = preflight.frame_count.max(0) as usize;
    let mut completed = 0usize;
    let mut after_id = 0i64;
    let mut all_ids = Vec::with_capacity(total);
    let mut failed_frame_ids = Vec::new();
    let mut failed_asset_ids = std::collections::HashSet::new();
    loop {
        if session.vector_rebuild_cancel.load(Ordering::SeqCst) {
            return Err(AppError::coded(
                "vector_rebuild_cancelled",
                "vector rebuild was cancelled",
            ));
        }
        let rows = {
            let conn = session.db.lock().unwrap();
            crate::db::assets::frames_full_after(&conn, &session.root, after_id, REBUILD_BATCH)?
        };
        if rows.is_empty() {
            break;
        }
        after_id = rows.last().map(|row| row.frame_id).unwrap_or(after_id);
        let mut readable_rows = Vec::with_capacity(rows.len());
        let mut images = Vec::with_capacity(rows.len());
        for row in &rows {
            match crate::import::thumb::open_image_smart(&row.path) {
                Ok(image) => {
                    readable_rows.push(row);
                    images.push(image);
                }
                Err(error) => {
                    failed_frame_ids.push(row.frame_id);
                    failed_asset_ids.insert(row.asset_id);
                    crate::logging::error(
                        "vector",
                        "rebuild_frame_decode_failed",
                        rebuild_id,
                        &format!(
                            "frame_id={};asset_id={};message={error}",
                            row.frame_id, row.asset_id
                        ),
                    );
                }
            }
        }
        let features = if images.is_empty() {
            Vec::new()
        } else {
            encode(&images)?
        };
        let points: Vec<_> = readable_rows
            .iter()
            .zip(features)
            .map(|(row, feature)| (row.frame_id, feature, frame_payload_json(row)))
            .collect();
        if !points.is_empty() {
            upsert_points_to_shard(&shard, points)?;
        }
        all_ids.extend(readable_rows.iter().map(|row| row.frame_id));
        completed += rows.len();
        if completed == total || completed % 1024 < rows.len() {
            crate::logging::info(
                "vector",
                "rebuild_progress",
                rebuild_id,
                &format!("completed={completed};total={total};last_frame_id={after_id}"),
            );
        }
        on_progress(RebuildProgress {
            library_id: session.meta.id.clone(),
            rebuild_id: rebuild_id.into(),
            phase: "building".into(),
            completed,
            total,
        });
    }
    if total > 0 && all_ids.is_empty() {
        return Err(AppError::coded(
            "vector_rebuild_verify_failed",
            "no readable frames were available for rebuilding",
        ));
    }
    shard
        .optimize()
        .map_err(|error| AppError::coded("vector_rebuild_optimize_failed", error.to_string()))?;
    shard
        .flush()
        .map_err(|error| AppError::coded("vector_rebuild_flush_failed", error.to_string()))?;
    drop(shard);

    let verified = open_index(&staging)?;
    for ids in all_ids.chunks(256) {
        let requested = ids
            .iter()
            .map(|id| PointId::NumId(*id as u64))
            .collect::<Vec<_>>();
        let records = verified
            .retrieve(RetrieveRequest::new(requested))
            .map_err(|error| AppError::coded("vector_rebuild_verify_failed", error.to_string()))?;
        if records.len() != ids.len() {
            return Err(AppError::coded(
                "vector_rebuild_verify_failed",
                format!("expected {} points, found {}", ids.len(), records.len()),
            ));
        }
    }
    drop(verified);
    journal.phase = RebuildPhase::Verified;
    journal.updated_at = crate::util::now();
    write_journal(&session.root, &journal)?;
    crate::logging::info(
        "vector",
        "rebuild_verified",
        rebuild_id,
        &format!(
            "points={};failed_frames={};staging_bytes={}",
            all_ids.len(),
            failed_frame_ids.len(),
            directory_disk_bytes(&staging)
        ),
    );

    // Release the current handle at the swap point; the old healthy index keeps serving
    // searches until then.
    drop(session.vector.lock().unwrap().take());
    let current = dirs.vector();
    if current.exists() {
        fs::rename(&current, &backup)?;
    }
    journal.phase = RebuildPhase::OldIndexMoved;
    journal.updated_at = crate::util::now();
    write_journal(&session.root, &journal)?;
    crate::logging::info(
        "vector",
        "rebuild_old_index_moved",
        rebuild_id,
        "rollback_available=true",
    );
    if let Err(error) = fs::rename(&staging, &current) {
        if backup.exists() && !current.exists() {
            let _ = fs::rename(&backup, &current);
        }
        return Err(error.into());
    }
    journal.phase = RebuildPhase::NewIndexMoved;
    journal.updated_at = crate::util::now();
    write_journal(&session.root, &journal)?;
    crate::logging::info(
        "vector",
        "rebuild_new_index_moved",
        rebuild_id,
        "reopen_required=true",
    );

    match open_index(&current) {
        Ok(new_shard) => *session.vector.lock().unwrap() = Some(new_shard),
        Err(error) => {
            let _ = fs::rename(&current, &staging);
            if backup.exists() {
                let _ = fs::rename(&backup, &current);
            }
            return Err(error);
        }
    }
    let database_result: AppResult<()> = (|| {
        let mut conn = session.db.lock().unwrap();
        let tx = conn.transaction()?;
        tx.execute("UPDATE frames SET embedded = 0", [])?;
        for frame_id in &all_ids {
            tx.execute(
                "UPDATE frames SET embedded = 1 WHERE id = ?1",
                rusqlite::params![frame_id],
            )?;
        }
        tx.execute(
            "UPDATE assets SET import_stage = 'done',visual_indexed=1 \
             WHERE deleted_at IS NULL AND (visual_indexed=1 OR (visual_requested=1 AND NOT EXISTS(SELECT 1 FROM index_tasks it WHERE it.asset_id=assets.id AND it.kind='visual'))) AND EXISTS (SELECT 1 FROM frames f WHERE f.asset_id = assets.id) \
             AND NOT EXISTS (SELECT 1 FROM frames f WHERE f.asset_id = assets.id AND f.embedded = 0)",
            [],
        )?;
        for asset_id in &failed_asset_ids {
            tx.execute(
                "UPDATE assets SET visual_indexed=0 WHERE id = ?1",
                rusqlite::params![asset_id],
            )?;
        }
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = database_result {
        // SQLite commit is part of the swap. On failure, release the new index and restore
        // old directories to avoid mixed points and readiness flags.
        drop(session.vector.lock().unwrap().take());
        let _ = fs::rename(&current, &staging);
        if backup.exists() {
            let _ = fs::rename(&backup, &current);
            if let Ok(old_shard) = open_index(&current) {
                *session.vector.lock().unwrap() = Some(old_shard);
            }
        }
        let _ = fs::remove_file(journal_path(&session.root));
        return Err(error);
    }
    let _ = fs::remove_file(journal_path(&session.root));
    if backup.exists() {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(RebuildReport {
        indexed_frames: all_ids.len(),
        failed_frames: failed_frame_ids.len(),
    })
}

#[cfg(test)]
#[path = "../../tests/unit/vector_recovery.rs"]
mod tests;
