//! Session-level asset organization and lifecycle operations shared by all transports.
//! SQLite changes precede vector payload synchronization; a synchronization error
//! does not roll back an already committed database or filesystem operation.

use std::fs;
use std::path::{Path, PathBuf};

use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession};
use crate::{util, vector};

mod file_moves;
mod journal;
pub(crate) use journal::{
    recover as recover_file_moves, recover_on_open as recover_file_moves_on_open,
};
mod permanent;
use file_moves::FileMoves;

/// Update logical folder membership without moving source files.
pub fn move_assets_to_folder(
    s: &LibrarySession,
    ids: &[i64],
    folder_id: Option<i64>,
) -> AppResult<()> {
    let mut conn = s.db.lock().unwrap();
    let tx = conn.transaction()?;
    if let Some(fid) = folder_id {
        if db::folders::get_name(&tx, fid)?.is_none() {
            return Err(AppError::coded(
                "folder_not_found",
                "target folder does not exist",
            ));
        }
    }
    for id in ids {
        db::assets::set_asset_folder(&tx, *id, folder_id)?;
    }
    // Publish vector payloads only after the entire SQLite batch commits.
    tx.commit()?;
    drop(conn);
    vector::sync_assets_payload(s, ids)
}

/// Soft-delete transactionally, compensating any managed-file moves before the whole metadata
/// batch commits.
pub fn trash_assets(s: &LibrarySession, ids: &[i64]) -> AppResult<()> {
    change_deleted_state(s, ids, LifecycleAction::Trash)
}

/// Restore existing folder membership without moving reference sources.
pub fn restore_assets(s: &LibrarySession, ids: &[i64]) -> AppResult<()> {
    change_deleted_state(s, ids, LifecycleAction::Restore)
}

#[derive(Clone, Copy)]
enum LifecycleAction {
    Trash,
    Restore,
}

fn change_deleted_state(s: &LibrarySession, ids: &[i64], action: LifecycleAction) -> AppResult<()> {
    let _cleanup = s.asset_activity.cleanup()?;
    recover_file_moves(s)?;
    let dirs = LibraryDirs::new(&s.root);
    let conn = s.db.lock().unwrap();
    let _sync = journal::FullSync::new(&conn)?;
    let mut journal = journal::Journal {
        committed: false,
        ids: ids.to_vec(),
        moves: Vec::new(),
    };
    let deleted_at = match action {
        LifecycleAction::Trash => Some(util::now()),
        LifecycleAction::Restore => None,
    };
    let mut planned = Vec::new();
    for id in ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let Some(asset) = db::assets::get_asset(&conn, &s.root, id)? else {
            continue;
        };
        if matches!(action, LifecycleAction::Restore) {
            db::asset_cleanup::ensure_not_pending(&conn, id)?;
        }
        if asset.deleted_at.is_some() == deleted_at.is_some() {
            continue;
        }
        let target = if asset.storage == "copy" {
            let (directory, name) = match action {
                LifecycleAction::Trash => {
                    (dirs.trash(), format!("{}_{}", asset.id, asset.file_name))
                }
                LifecycleAction::Restore => (
                    dirs.asset(asset.imported_at, &asset.uid),
                    asset.file_name.clone(),
                ),
            };
            fs::create_dir_all(&directory)?;
            let target = unique_target(&directory, &name);
            journal.moves.push(journal::Journal::plan(
                &s.root,
                Path::new(&asset.abs_path),
                &target,
            )?);
            Some(target)
        } else {
            None
        };
        planned.push((asset, target));
    }
    let key = journal::Journal::key();
    // Persist the whole batch before any original file is removed.
    journal.save(&conn, &key)?;
    s.asset_activity.set_recovery_required(true);
    let mut moves = FileMoves::default();
    let tx = conn.unchecked_transaction()?;
    let result: AppResult<()> = (|| {
        for (asset, target) in &planned {
            let id = asset.id;
            if let Some(target) = target {
                moves.move_to(Path::new(&asset.abs_path), target)?;
                let rel = util::to_rel(&s.root, target);
                // Trash's ID prefix is a physical path detail, not the display filename.
                let file_name = match action {
                    LifecycleAction::Trash => asset.file_name.clone(),
                    LifecycleAction::Restore => {
                        target.file_name().unwrap().to_string_lossy().into_owned()
                    }
                };
                db::assets::set_asset_rel_path(&tx, id, &rel, &file_name)?;
            }
            if matches!(action, LifecycleAction::Restore) {
                let folder = match asset.folder_id {
                    Some(id) if db::folders::get_name(&tx, id)?.is_some() => Some(id),
                    _ => None,
                };
                db::assets::set_asset_folder(&tx, id, folder)?;
            }
            db::assets::set_deleted(&tx, &[id], deleted_at)?;
        }
        journal.committed = true;
        journal.save(&tx, &key)?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(cause) = result {
        return match moves.rollback() {
            Ok(()) => {
                drop(_sync);
                drop(conn);
                recover_file_moves(s)?;
                Err(cause)
            }
            Err(rollback) => Err(AppError::coded(
                "asset_rollback_failed",
                format!("{cause}; {rollback}"),
            )),
        };
    }
    moves.committed();
    // The committed file phase is complete; retain only payload-sync intent.
    journal.moves.clear();
    journal.save(&conn, &key)?;
    s.asset_activity.set_recovery_required(false);
    drop(_sync);
    drop(conn);
    // A vector failure is post-commit. Repeating the request skips file moves
    // but retries synchronization for every requested asset.
    vector::sync_assets_payload(s, ids)?;
    let conn = s.db.lock().unwrap();
    let _sync = journal::FullSync::new(&conn)?;
    journal::Journal::clear(&conn, &key)?;
    s.asset_activity.set_recovery_required(false);
    Ok(())
}

/// Persist permanent-deletion intent and remove records only after all cleanup succeeds.
pub fn delete_permanent(s: &LibrarySession, ids: &[i64]) -> AppResult<()> {
    permanent::delete_permanent(s, ids)
}

/// Move with rename, falling back to copy/delete across volumes.
pub fn move_file(src: &str, dest: &Path) -> AppResult<()> {
    if fs::rename(src, dest).is_err() {
        fs::copy(src, dest)?;
        fs::remove_file(src)?;
    }
    Ok(())
}

/// Choose a unique destination filename with a numbered suffix.
pub fn unique_target(dir: &std::path::Path, file_name: &str) -> PathBuf {
    let cand = dir.join(file_name);
    if !cand.exists() {
        return cand;
    }
    let (stem, ext) = match file_name.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), e.to_string()),
        None => (file_name.to_string(), String::new()),
    };
    for n in 1.. {
        let name = if ext.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}).{ext}")
        };
        let cand = dir.join(name);
        if !cand.exists() {
            return cand;
        }
    }
    unreachable!()
}
