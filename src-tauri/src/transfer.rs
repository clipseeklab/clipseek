//! Cross-library transfer rebuilds folder/tag mappings and copies only managed and derived
//! files. It reads source vectors into new destination point IDs; missing vectors remain
//! pending for destination indexing. Reference source files are never copied or deleted.

mod copy;
mod duplicates;
mod storage;
use copy::copy_asset_to_session_outcome;
use storage::{directory_size, ensure_transfer_space};

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::GlobalConfig;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::{self, LibraryDirs, LibrarySession};
use crate::services::folders as folder_svc;
use crate::util;

enum CopyAssetOutcome {
    Copied(i64),
    Existing(i64),
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferSummary {
    pub assets: usize,
    pub failed: usize,
    pub folders: usize,
    /// Count assets skipped because complete matching content already exists in the
    /// destination.
    pub skipped: usize,
    pub operation_id: String,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub operation_id: String,
    pub operation_type: String,
    pub status: String,
    pub phase: String,
    pub total_items: usize,
    pub completed_items: usize,
    pub total_bytes: u64,
    pub completed_bytes: u64,
    pub current_item: Option<String>,
    pub failed: usize,
    pub skipped: usize,
    pub message: Option<String>,
}

impl TransferProgress {
    pub(crate) fn save(&self, root: &Path) -> AppResult<()> {
        self.save_in(&root.join(".clipstudio-open/operations"))
    }

    pub(crate) fn save_in(&self, dir: &Path) -> AppResult<()> {
        fs::create_dir_all(dir)?;
        cleanup_operation_files(dir);
        let path = dir.join(format!("{}.json", self.operation_id));
        // Remove journals for completed operations; retain failed or interrupted records for
        // restart recovery.
        if self.status == "completed" {
            if path.exists() {
                fs::remove_file(path)?;
            }
            return Ok(());
        }
        let tmp = path.with_extension("json.tmp");
        let mut bounded = self.clone();
        if let Some(message) = bounded.message.as_mut() {
            message.truncate(4096);
        }
        fs::write(&tmp, serde_json::to_vec(&bounded)?)?;
        fs::rename(tmp, path)?;
        Ok(())
    }
}

fn cleanup_operation_files(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    let mut files: Vec<_> = entries
        .flatten()
        .filter_map(|entry| {
            let meta = entry.metadata().ok()?;
            if !meta.is_file() || entry.path().extension().and_then(|x| x.to_str()) != Some("json")
            {
                return None;
            }
            let modified = meta.modified().ok()?;
            if meta.len() > 64 * 1024
                || now.duration_since(modified).unwrap_or_default()
                    > std::time::Duration::from_secs(72 * 3600)
            {
                let _ = fs::remove_file(entry.path());
                return None;
            }
            Some((modified, entry.path()))
        })
        .collect();
    files.sort_by_key(|x| std::cmp::Reverse(x.0));
    for (_, path) in files.into_iter().skip(10) {
        let _ = fs::remove_file(path);
    }
}

/// Copy one asset to the destination, or merge metadata into a complete duplicate.
pub fn copy_asset_to_session(
    src: &LibrarySession,
    asset_id: i64,
    dst: &LibrarySession,
    folder_id_in_dst: Option<i64>,
) -> AppResult<i64> {
    Ok(
        match copy_asset_to_session_outcome(src, asset_id, dst, folder_id_in_dst)? {
            CopyAssetOutcome::Copied(id) | CopyAssetOutcome::Existing(id) => id,
        },
    )
}

fn ensure_empty_source_trash(conn: &rusqlite::Connection) -> AppResult<()> {
    let trash_count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE deleted_at IS NOT NULL",
        [],
        |row| row.get(0),
    )?;
    if trash_count > 0 {
        return Err(AppError::LibraryState(format!(
            "The source library has {trash_count} trashed assets. Restore or permanently delete them before merging; the source library was retained"
        )));
    }
    Ok(())
}

/// Merge all source assets into target, removing the source registration and directory only
/// after full success. Reuse active sessions because Qdrant permits one WAL handle per
/// directory.
pub fn merge_library(
    config: &mut GlobalConfig,
    source_id: &str,
    target_id: &str,
    open_sessions: &HashMap<String, std::sync::Arc<LibrarySession>>,
) -> AppResult<TransferSummary> {
    merge_library_with_progress(config, source_id, target_id, open_sessions, &|_| {})
}

pub fn merge_library_with_progress(
    config: &mut GlobalConfig,
    source_id: &str,
    target_id: &str,
    open_sessions: &HashMap<String, std::sync::Arc<LibrarySession>>,
    on_progress: &dyn Fn(&TransferProgress),
) -> AppResult<TransferSummary> {
    merge_library_with_progress_and_close(
        config,
        source_id,
        target_id,
        open_sessions,
        on_progress,
        || {},
    )
}

pub(crate) fn merge_library_with_progress_and_close(
    config: &mut GlobalConfig,
    source_id: &str,
    target_id: &str,
    open_sessions: &HashMap<String, std::sync::Arc<LibrarySession>>,
    on_progress: &dyn Fn(&TransferProgress),
    close_source: impl FnOnce(),
) -> AppResult<TransferSummary> {
    if source_id == target_id {
        return Err(AppError::coded(
            "library_same_as_source",
            "source and target libraries are the same",
        ));
    }
    let source_meta = config
        .library_by_id(source_id)
        .cloned()
        .ok_or_else(|| AppError::LibraryNotFound)?;
    let target_meta = config
        .library_by_id(target_id)
        .cloned()
        .ok_or_else(|| AppError::LibraryNotFound)?;

    library::check_compatible(Path::new(&source_meta.path))?;
    library::check_compatible(Path::new(&target_meta.path))?;
    // Reject a nonempty trash before either database can be upgraded.
    let source_needs_upgrade = {
        let path = LibraryDirs::new(Path::new(&source_meta.path)).db();
        let conn = db::open_library_read_db(&path)?;
        ensure_empty_source_trash(&conn)?;
        conn.query_row(
            "SELECT COALESCE((SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'),0) < ?1",
            [db::SCHEMA_VERSION],
            |row| row.get::<_, bool>(0),
        )?
    };
    // Prefer existing sessions to satisfy the one-open-WAL constraint.
    let src = match open_sessions.get(source_id) {
        Some(s) => s.clone(),
        // Normal writable opening backs up and upgrades legacy sources before copying.
        None => std::sync::Arc::new(library::open_session_with_close(source_meta, close_source)?),
    };
    let dst = match open_sessions.get(target_id) {
        Some(s) => s.clone(),
        None => std::sync::Arc::new(library::open_session(target_meta)?),
    };
    // Block merging when source trash remains; removing the source directory must not discard
    // assets the user has not permanently deleted.
    ensure_empty_source_trash(&src.db.lock().unwrap())?;
    let source_retained = if source_needs_upgrade {
        "Source library retained and format upgraded; the pre-upgrade database backup is in its .clipstudio-open/backups directory"
    } else {
        "Source library retained"
    };
    let operation_id = util::new_entity_uid();
    let mut summary = TransferSummary {
        operation_id: operation_id.clone(),
        ..Default::default()
    };

    // Map folders, reusing matching destination names.
    let mut folder_map: HashMap<i64, i64> = HashMap::new();
    {
        let src_folders = {
            let conn = src.db.lock().unwrap();
            db::folders::list(&conn)?
        };
        for f in src_folders {
            let existing = {
                let conn = dst.db.lock().unwrap();
                db::folders::list(&conn)?
                    .into_iter()
                    .find(|t| t.name == f.name)
            };
            let tid = match existing {
                Some(t) => t.id,
                None => folder_svc::create_folder(&dst, &f.name)?.id,
            };
            folder_map.insert(f.id, tid);
            summary.folders += 1;
        }
    }

    // Transfer assets one at a time.
    let rows: Vec<(i64, Option<i64>)> = {
        let conn = src.db.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, folder_id FROM assets WHERE deleted_at IS NULL ORDER BY id")?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut planned_rows = Vec::with_capacity(rows.len());
    for (id, folder_id) in rows {
        let asset = {
            let conn = src.db.lock().unwrap();
            db::assets::get_asset(&conn, &src.root, id)?.ok_or_else(|| AppError::AssetNotFound)?
        };
        // References may point into huge external directories; count only library-owned
        // derived files, not source parents or source bytes.
        let item_bytes = if asset.storage == "reference" {
            let dirs = LibraryDirs::new(&src.root);
            directory_size(&dirs.asset(asset.imported_at, &asset.uid)).unwrap_or(0)
        } else {
            let parent = Path::new(&asset.abs_path).parent().ok_or_else(|| {
                AppError::coded(
                    "asset_path_invalid",
                    format!("asset path invalid: {}", asset.abs_path),
                )
            })?;
            directory_size(parent)?
        };
        planned_rows.push((id, folder_id, item_bytes));
    }
    let total_bytes: u64 = planned_rows.iter().map(|(_, _, bytes)| *bytes).sum();
    let mut progress = TransferProgress {
        operation_id,
        operation_type: "merge_library".into(),
        status: "planning".into(),
        phase: "checking_space".into(),
        total_items: planned_rows.len(),
        completed_items: 0,
        total_bytes,
        completed_bytes: 0,
        current_item: None,
        failed: 0,
        skipped: 0,
        message: None,
    };
    progress.save(&dst.root)?;
    on_progress(&progress);
    if let Err(error) = ensure_transfer_space(&dst.root, total_bytes) {
        progress.status = "failed".into();
        progress.message = Some(format!(
            "Insufficient destination space; {source_retained}. Free space or choose another destination and retry"
        ));
        progress.save(&dst.root)?;
        on_progress(&progress);
        return Err(error);
    }
    progress.status = "running".into();
    progress.phase = "copying_files".into();
    for (id, src_folder, item_bytes) in planned_rows {
        let item = {
            let conn = src.db.lock().unwrap();
            db::assets::get_asset(&conn, &src.root, id)?
        };
        progress.current_item = item.as_ref().map(|a| a.file_name.clone());
        on_progress(&progress);
        let dst_folder = src_folder.and_then(|fid| folder_map.get(&fid).copied());
        match copy_asset_to_session_outcome(&src, id, &dst, dst_folder) {
            Ok(CopyAssetOutcome::Copied(_)) => summary.assets += 1,
            Ok(CopyAssetOutcome::Existing(_)) => summary.skipped += 1,
            Err(e) => {
                summary.failed += 1;
                summary.errors.push(format!(
                    "{}: {e}",
                    progress.current_item.as_deref().unwrap_or("Unknown asset")
                ));
                eprintln!("Failed to merge asset {id}: {e}");
            }
        }
        progress.completed_items += 1;
        progress.completed_bytes = progress.completed_bytes.saturating_add(item_bytes);
        progress.failed = summary.failed;
        progress.skipped = summary.skipped;
        progress.save(&dst.root)?;
        on_progress(&progress);
    }

    // Optimize the destination shard.
    progress.phase = "writing_vectors".into();
    on_progress(&progress);
    {
        let shard = dst.vector.lock().unwrap();
        if let Some(shard) = shard.as_ref() {
            let _ = shard.optimize();
        }
    }

    // Remove the source only after all transfers succeed; otherwise preserve it for retry.
    if summary.failed > 0 {
        progress.status = "failed".into();
        progress.message = Some(format!(
            "{} assets failed; {source_retained}. Correct the problem and retry",
            summary.failed
        ));
        progress.save(&dst.root)?;
        on_progress(&progress);
        return Err(AppError::Msg(format!(
            "Merge finished with {} failures and {} successes; {source_retained}",
            summary.failed, summary.assets
        )));
    }
    let src_is_active = open_sessions.contains_key(source_id);
    if src_is_active {
        // Preserve compatibility for non-UI callers without forcibly closing externally held
        // sessions.
        library::delete_library(config, source_id, false)?;
    } else {
        drop(src);
        library::delete_library(config, source_id, true)?;
    }
    progress.status = "completed".into();
    progress.phase = "completed".into();
    progress.current_item = None;
    progress.save(&dst.root)?;
    on_progress(&progress);
    Ok(summary)
}

/// Transfer to the destination's uncategorized collection, then remove source records safely.
pub fn move_assets_to_library(
    state: &crate::AppState,
    ids: &[i64],
    target_library_id: &str,
) -> AppResult<usize> {
    move_assets_to_library_with_progress(state, ids, target_library_id, &|_| {})
}

/// Commit each destination before deleting source records. Stable UIDs and content hashes
/// make retries of interrupted batches idempotent.
pub fn move_assets_to_library_with_progress(
    state: &crate::AppState,
    ids: &[i64],
    target_library_id: &str,
    on_progress: &dyn Fn(&TransferProgress),
) -> AppResult<usize> {
    let source = crate::services::sessions::current(state)?;
    move_assets_from_session_with_progress(state, source, ids, target_library_id, on_progress)
}

pub(crate) fn move_assets_from_session_with_progress(
    state: &crate::AppState,
    src_arc: std::sync::Arc<LibrarySession>,
    ids: &[i64],
    target_library_id: &str,
    on_progress: &dyn Fn(&TransferProgress),
) -> AppResult<usize> {
    let target_meta = state
        .config
        .read()
        .unwrap()
        .library_by_id(target_library_id)
        .cloned()
        .ok_or(AppError::LibraryNotFound)?;
    if target_meta.id == src_arc.meta.id {
        return Err(AppError::coded(
            "library_same_as_source",
            "target library is the current library",
        ));
    }
    let dst = crate::services::sessions::resolve(state, Some(&target_meta.id))?;
    let mut items: Vec<(i64, Option<db::model::Asset>, u64)> = Vec::with_capacity(ids.len());
    for id in ids {
        let asset = src_arc.db.lock().ok().and_then(|conn| {
            db::assets::get_asset(&conn, &src_arc.root, *id)
                .ok()
                .flatten()
        });
        let bytes = match asset.as_ref() {
            Some(asset) if db::asset_cleanup::pending(&src_arc.read_db(), &asset.uid)? => 0,
            // Copy only derived directories for references, never the external source.
            Some(asset) if asset.storage == "reference" => {
                let dirs = LibraryDirs::new(&src_arc.root);
                directory_size(&dirs.asset(asset.imported_at, &asset.uid)).unwrap_or(0)
            }
            Some(asset) => Path::new(&asset.abs_path)
                .parent()
                .map(directory_size)
                .unwrap_or(Ok(0))?,
            None => 0,
        };
        items.push((*id, asset, bytes));
    }
    let total_bytes = items.iter().map(|(_, _, bytes)| *bytes).sum();
    let mut progress = TransferProgress {
        operation_id: util::new_entity_uid(),
        operation_type: "move_assets".into(),
        status: "planning".into(),
        phase: "checking_space".into(),
        total_items: items.len(),
        completed_items: 0,
        total_bytes,
        completed_bytes: 0,
        current_item: None,
        failed: 0,
        skipped: 0,
        message: None,
    };
    progress.save(&dst.root)?;
    on_progress(&progress);
    if let Err(error) = ensure_transfer_space(&dst.root, total_bytes) {
        progress.status = "failed".into();
        progress.message =
            Some("Insufficient destination space; all untransferred source assets were retained. Free space and retry".into());
        progress.save(&dst.root)?;
        on_progress(&progress);
        return Err(error);
    }
    progress.status = "running".into();
    progress.phase = "copying_files".into();
    let mut moved = 0usize;
    let mut errors = Vec::new();
    for (id, asset, bytes) in items {
        progress.current_item = asset.as_ref().map(|asset| asset.file_name.clone());
        on_progress(&progress);
        let copy_result = match duplicates::resume_cleanup(&src_arc, id, &dst) {
            Ok(true) => {
                moved += 1;
                progress.skipped += 1;
                progress.completed_items += 1;
                progress.save(&dst.root)?;
                on_progress(&progress);
                continue;
            }
            Err(error) => Err(error),
            Ok(false) => copy_asset_to_session_outcome(&src_arc, id, &dst, None),
        };
        let copy_result = match copy_result {
            Ok(CopyAssetOutcome::Copied(id)) => Ok((false, id)),
            Ok(CopyAssetOutcome::Existing(id)) => Ok((true, id)),
            Err(error) => Err(error),
        };
        match copy_result.and_then(|(skipped, target_id)| {
            let _target = dst.asset_activity.work()?;
            if !duplicates::reusable_target(
                &dst,
                target_id,
                &src_arc,
                asset.as_ref().ok_or(AppError::AssetNotFound)?,
            )? {
                return Err(AppError::coded(
                    "library_file_missing",
                    "The target is incomplete; source has been retained",
                ));
            }
            crate::services::assets::delete_permanent(&src_arc, &[id])?;
            Ok(skipped)
        }) {
            Ok(skipped) => {
                moved += 1;
                if skipped {
                    progress.skipped += 1;
                }
            }
            Err(error) => {
                progress.failed += 1;
                errors.push(format!(
                    "{}: {error}",
                    progress.current_item.as_deref().unwrap_or("Unknown asset")
                ));
            }
        }
        progress.completed_items += 1;
        progress.completed_bytes = progress.completed_bytes.saturating_add(bytes);
        progress.save(&dst.root)?;
        on_progress(&progress);
    }
    if !errors.is_empty() {
        progress.status = "failed".into();
        progress.message = Some(format!(
            "{} asset transfers failed; their source files were retained. Correct the problem and retry. {}",
            errors.len(),
            errors.join("；")
        ));
        progress.save(&dst.root)?;
        on_progress(&progress);
        return Err(AppError::Msg(progress.message.unwrap()));
    }
    progress.status = "completed".into();
    progress.phase = "completed".into();
    progress.current_item = None;
    progress.save(&dst.root)?;
    on_progress(&progress);
    Ok(moved)
}

#[cfg(test)]
#[path = "../tests/unit/transfer_safety.rs"]
mod safety_tests;
