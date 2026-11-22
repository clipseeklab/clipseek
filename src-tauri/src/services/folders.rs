//! Transport-neutral logical folder operations. Paths of source files do not
//! depend on folder names; deletion delegates asset lifecycle work to its service.

use std::collections::BTreeSet;

use crate::db::{self, model::Folder};
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::util;

use super::assets;

/// Validate a logical folder name.
pub(crate) fn validate_folder_name(name: &str) -> AppResult<String> {
    let name = name.trim().to_string();
    if name.is_empty() || name.contains('/') || name.contains(':') || name.starts_with('.') {
        return Err(AppError::InvalidArgument(
            "Folder names must not be empty, contain / or :, or start with a dot".into(),
        ));
    }
    Ok(name)
}

/// Create a logical folder in SQLite only.
pub fn create_folder(s: &LibrarySession, name: &str) -> AppResult<Folder> {
    let now = util::now();
    let mut conn = s.db.lock().unwrap();
    let tx = conn.transaction()?;
    let sidebar = db::folders::sidebar(&tx)?;
    let id = db::folders::insert(&tx, name, now)?;
    // Count all folders before creation, preserving manually unpinned entries.
    // Folder creation and its default shortcut must commit together.
    if sidebar.total < sidebar.display_limit {
        db::folders::pin(&tx, id, true)?;
    }
    tx.commit()?;
    Ok(Folder {
        id,
        name: name.to_string(),
        created_at: now,
        asset_count: 0,
    })
}

/// Rename a logical folder without changing asset paths.
pub fn rename_folder(s: &LibrarySession, id: i64, new_name: &str) -> AppResult<()> {
    let conn = s.db.lock().unwrap();
    let old_name = db::folders::get_name(&conn, id)?
        .ok_or_else(|| AppError::coded("folder_not_found", "folder not found"))?;
    if old_name == new_name {
        return Ok(());
    }
    db::folders::rename(&conn, id, new_name)?;
    Ok(())
}

/// Trash member assets before deleting the logical folder.
pub fn delete_folder(s: &LibrarySession, id: i64) -> AppResult<()> {
    // Trash all live assets in this folder.
    let asset_ids: Vec<i64> = {
        let conn = s.db.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id FROM assets WHERE folder_id = ?1 AND deleted_at IS NULL")?;
        let rows = stmt
            .query_map([id], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    assets::trash_assets(s, &asset_ids)?;

    let conn = s.db.lock().unwrap();
    if db::folders::get_name(&conn, id)?.is_none() {
        return Err(AppError::coded("folder_not_found", "folder not found"));
    }
    // Clear folder IDs on trashed assets so restores become uncategorized.
    db::folders::delete(&conn, id)?;
    Ok(())
}

/// Validate all selected folders before moving their live assets or deleting
/// the folders. Moves commit as one asset batch; deletions commit per folder.
pub fn batch(s: &LibrarySession, ids: &[i64], target_id: Option<i64>) -> AppResult<()> {
    if ids.is_empty() || ids.len() > 1000 || target_id.is_some_and(|id| ids.contains(&id)) {
        return Err(AppError::InvalidArgument("Invalid folder selection".into()));
    }
    let ids = ids.iter().copied().collect::<BTreeSet<_>>();
    let asset_ids = {
        let conn = s.db.lock().unwrap();
        for id in ids.iter().copied().chain(target_id) {
            if db::folders::get_name(&conn, id)?.is_none() {
                return Err(AppError::coded("folder_not_found", "folder not found"));
            }
        }
        let mut asset_ids = Vec::new();
        for id in &ids {
            let mut query =
                conn.prepare("SELECT id FROM assets WHERE folder_id=?1 AND deleted_at IS NULL")?;
            asset_ids.extend(
                query
                    .query_map([id], |r| r.get::<_, i64>(0))?
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }
        asset_ids
    };
    if let Some(target) = target_id {
        assets::move_assets_to_folder(s, &asset_ids, Some(target))?;
    } else {
        for id in ids {
            delete_folder(s, id)?;
        }
    }
    Ok(())
}
