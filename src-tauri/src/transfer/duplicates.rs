//! Reuse completed target metadata only when permanent cleanup has not started.
use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::vector;

pub(super) fn merge_duplicate_metadata(
    dst: &LibrarySession,
    target_id: i64,
    source: &db::model::Asset,
    folder_id_in_dst: Option<i64>,
) -> AppResult<()> {
    {
        let conn = dst.db.lock().unwrap();
        db::asset_cleanup::ensure_not_pending(&conn, target_id)?;
        db::tags::add_to_asset(&conn, target_id, &source.tags)?;
        let current_folder: Option<i64> = conn.query_row(
            "SELECT folder_id FROM assets WHERE id=?1",
            [target_id],
            |row| row.get(0),
        )?;
        if current_folder.is_none() && folder_id_in_dst.is_some() {
            db::assets::set_asset_folder(&conn, target_id, folder_id_in_dst)?;
        }
        if let Some(url) = source.web_source_url.as_deref() {
            db::assets::set_web_source(
                &conn,
                target_id,
                url,
                source.web_source_title.as_deref().unwrap_or_default(),
            )?;
        }
        if source.display_name.is_some() {
            let current: Option<String> = conn.query_row(
                "SELECT display_name FROM assets WHERE id=?1",
                [target_id],
                |row| row.get(0),
            )?;
            if current.is_none() {
                db::assets::set_display_name(&conn, target_id, source.display_name.as_deref())?;
            }
        }
    }
    // Folder/tag payload synchronization failures must be reported to avoid inconsistent
    // search state.
    vector::sync_asset_payload(dst, target_id)
}

/// A duplicate row is not evidence that the source can safely be removed.
pub(super) fn reusable_target(
    dst: &LibrarySession,
    id: i64,
    src: &LibrarySession,
    source: &db::model::Asset,
) -> AppResult<bool> {
    let asset = {
        let conn = dst.read_db();
        db::asset_cleanup::ensure_not_pending(&conn, id)?;
        db::assets::get_asset(&conn, &dst.root, id)?.ok_or(AppError::AssetNotFound)?
    };
    if asset.deleted_at.is_some() {
        return Err(AppError::coded(
            "asset_in_trash",
            "The duplicate target is in the trash; restore it before transferring",
        ));
    }

    if !matches!(asset.import_stage.as_str(), "done" | "embedding") {
        return Ok(false);
    }
    // Reference-to-reference transfer preserves the same external source, even offline.
    if asset.storage == "reference"
        && source.storage == "reference"
        && asset.source_path == source.source_path
    {
        return Ok(true);
    }
    let Ok(path) = std::path::Path::new(&asset.abs_path).canonicalize() else {
        return Ok(false);
    };
    if !path.is_file() {
        return Ok(false);
    }
    if asset.storage == "reference" {
        // A reference into the library being removed is not an independent copy.
        return Ok(!path.starts_with(src.root.canonicalize()?));
    }
    Ok(path.starts_with(dst.root.canonicalize()?))
}

/// Resume only after independently validating an existing committed target.
pub(super) fn resume_cleanup(
    src: &LibrarySession,
    id: i64,
    dst: &LibrarySession,
) -> AppResult<bool> {
    let (source, hash) = {
        let conn = src.read_db();
        let Some(asset) = db::assets::get_asset(&conn, &src.root, id)? else {
            return Ok(false);
        };
        if !db::asset_cleanup::pending(&conn, &asset.uid)? {
            return Ok(false);
        }
        let hash: String =
            conn.query_row("SELECT file_hash FROM assets WHERE id=?1", [id], |r| {
                r.get(0)
            })?;
        (asset, hash)
    };
    let _target = dst.asset_activity.work()?;
    let target = {
        let conn = dst.read_db();
        match db::assets::find_by_uid(&conn, &source.uid)? {
            Some((id, target_hash)) if target_hash == hash => Some(id),
            Some(_) => None,
            None => db::assets::find_by_hash(&conn, &hash)?,
        }
    };
    if !target
        .map(|id| reusable_target(dst, id, src, &source))
        .transpose()?
        .unwrap_or(false)
    {
        return Err(AppError::coded(
            "library_file_missing",
            "No complete target exists for pending source cleanup",
        ));
    }
    crate::services::assets::delete_permanent(src, &[id])?;
    Ok(true)
}
