//! Cleanup before final row deletion: failures remain visible and retryable after reopen.
use std::fs;
use std::io;
use std::path::{Component, Path};

use crate::db::{self, model::Asset};
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession};
use crate::{util, vector};

pub(super) fn delete_permanent(session: &LibrarySession, ids: &[i64]) -> AppResult<()> {
    let mut failures = Vec::new();
    for id in ids
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        if let Err(error) = purge_one(session, id) {
            failures.push(format!("asset {id}: {error}"));
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::coded("asset_cleanup_failed", failures.join("; ")))
    }
}

fn purge_one(session: &LibrarySession, id: i64) -> AppResult<()> {
    {
        let mut conn = session.db.lock().unwrap();
        let tx = conn.transaction()?;
        let Some(asset) = db::assets::get_asset(&tx, &session.root, id)? else {
            return Ok(());
        };
        db::asset_cleanup::mark(&tx, &asset.uid)?;
        // Cancellation and accounting participate in the same durable intent.
        if asset.deleted_at.is_none() {
            db::assets::set_deleted(&tx, &[id], Some(util::now()))?;
        }
        tx.commit()?;
    }
    // Intent is committed before checking activity, so an in-flight worker can
    // observe cancellation. Never wait for it while holding a database mutex.
    let _cleanup = session.asset_activity.cleanup()?;
    super::recover_file_moves(session)?;
    // Read paths and frame IDs after exclusive cleanup ownership is acquired:
    // a worker could have completed a final write after the intent committed.
    let (asset, frame_ids) = {
        let conn = session.db.lock().unwrap();
        let Some(asset) = db::assets::get_asset(&conn, &session.root, id)? else {
            return Ok(());
        };
        let frames = db::assets::get_frame_ids(&conn, id)?
            .into_iter()
            .map(|(id, _)| id)
            .collect::<Vec<_>>();
        (asset, frames)
    };
    let directory = LibraryDirs::new(&session.root).asset(asset.imported_at, &asset.uid);
    let mut files = Vec::new();
    if asset.storage == "copy" {
        files.push(Path::new(&asset.abs_path));
    }
    if let Some(thumb) = &asset.thumb_path {
        files.push(Path::new(thumb));
    }
    // Validate every target before deleting anything, especially reference sources.
    for path in files
        .iter()
        .copied()
        .chain(std::iter::once(directory.as_path()))
    {
        validate_target(&session.root, &asset, path)?;
    }
    // Remove searchable points before file cleanup, while retaining their IDs
    // for an idempotent retry if either phase fails.
    vector::delete_points(session, &frame_ids)?;
    for file in files {
        remove_if_present(file, |path| fs::remove_file(path))?;
    }
    remove_if_present(&directory, |path| fs::remove_dir_all(path))?;
    let mut conn = session.db.lock().unwrap();
    let tx = conn.transaction()?;
    db::assets::delete_assets(&tx, &[id])?;
    db::asset_cleanup::clear(&tx, &asset.uid)?;
    tx.commit()?;
    Ok(())
}

fn remove_if_present(path: &Path, remove: impl FnOnce(&Path) -> io::Result<()>) -> AppResult<()> {
    match remove(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(AppError::coded(
            "asset_cleanup_failed",
            format!("cannot remove {}: {error}", path.display()),
        )),
    }
}

fn validate_target(root: &Path, asset: &Asset, target: &Path) -> AppResult<()> {
    if !target.starts_with(root)
        || target
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(AppError::coded(
            "asset_cleanup_path_invalid",
            format!("cleanup target outside library: {}", target.display()),
        ));
    }
    // Canonicalize the closest existing ancestor as well, so a missing leaf
    // cannot hide an ancestor symlink pointing outside the library.
    let mut ancestor = target;
    let resolved = loop {
        match ancestor.canonicalize() {
            Ok(path) => break path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                ancestor = ancestor.parent().ok_or_else(|| AppError::Io(error))?;
            }
            Err(error) => return Err(error.into()),
        }
    };
    if !resolved.starts_with(root.canonicalize()?) {
        return Err(AppError::coded(
            "asset_cleanup_path_invalid",
            format!(
                "cleanup target resolves outside library: {}",
                target.display()
            ),
        ));
    }
    if asset.storage == "reference" {
        let source = Path::new(&asset.abs_path);
        if source == target
            || source.starts_with(target)
            || source.canonicalize().ok().is_some_and(|source| {
                source == resolved || (target.is_dir() && source.starts_with(&resolved))
            })
        {
            return Err(AppError::coded(
                "asset_cleanup_path_invalid",
                format!(
                    "cleanup target overlaps reference source: {}",
                    target.display()
                ),
            ));
        }
    }
    Ok(())
}
