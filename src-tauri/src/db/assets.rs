//! Asset writes and the public facade for asset queries and keyframes.

use std::path::Path;

use rusqlite::{params, Connection};

use super::model::Asset;
use crate::error::{AppError, AppResult};

/// Covers may only reference an existing image among this video's first ten frames.
pub fn set_video_cover(conn: &Connection, root: &Path, id: i64, frame_id: i64) -> AppResult<Asset> {
    let asset = get_asset(conn, root, id)?
        .filter(|a| a.deleted_at.is_none())
        .ok_or(AppError::AssetNotFound)?;
    if asset.kind != "video" {
        return Err(AppError::AssetTypeUnsupported);
    }
    let mut stmt = conn.prepare(
        "SELECT id, rel_path FROM frames WHERE asset_id = ?1 ORDER BY seq ASC, id ASC LIMIT 10",
    )?;
    let frames = stmt
        .query_map([id], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, Option<String>>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let rel = frames
        .into_iter()
        .find(|(candidate, _)| *candidate == frame_id)
        .ok_or_else(|| AppError::InvalidArgument("Choose one of the first ten keyframes".into()))?
        .1;
    let unavailable = || {
        AppError::coded(
            "cover_frame_unavailable",
            "The selected keyframe is unavailable",
        )
    };
    let rel = rel
        .filter(|value| !value.is_empty())
        .ok_or_else(unavailable)?;
    let path = root.join(&rel).canonicalize().map_err(|_| unavailable())?;
    let canonical_root = root.canonicalize()?;
    if !path.starts_with(&canonical_root)
        || !path.is_file()
        || image::image_dimensions(&path).is_err()
    {
        return Err(unavailable());
    }
    conn.execute(
        "UPDATE assets SET thumb_rel = ?1, cover_manual = 1 WHERE id = ?2",
        params![rel, id],
    )?;
    get_asset(conn, root, id)?.ok_or(AppError::AssetNotFound)
}

/// Read-only source libraries used by transfer may still have the v17 schema.
pub fn has_manual_cover(conn: &Connection, id: i64) -> AppResult<bool> {
    let has_column: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info('assets') WHERE name='cover_manual')",
        [],
        |row| row.get(0),
    )?;
    let sql = if has_column {
        "SELECT cover_manual FROM assets WHERE id=?1"
    } else {
        "SELECT EXISTS(SELECT 1 FROM frames f JOIN assets a ON a.id=f.asset_id WHERE a.id=?1 AND a.kind='video' AND a.thumb_rel=f.rel_path)"
    };
    Ok(conn.query_row(sql, [id], |row| row.get(0))?)
}

pub fn set_web_source(conn: &Connection, id: i64, url: &str, title: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET web_source_url = COALESCE(web_source_url, ?1), \
         web_source_title = COALESCE(web_source_title, ?2), \
         display_name = COALESCE(display_name, NULLIF(trim(?2), '')) WHERE id = ?3",
        params![url, title, id],
    )?;
    Ok(())
}

pub fn set_display_name(conn: &Connection, id: i64, display_name: Option<&str>) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET display_name = ?1 WHERE id = ?2",
        params![display_name, id],
    )?;
    Ok(())
}

/// Data required to register a managed or referenced asset.
#[derive(Default)]
pub struct NewAsset<'a> {
    pub uid: Option<&'a str>,
    pub folder_id: Option<i64>,
    pub file_name: &'a str,
    pub rel_path: &'a str,
    pub file_hash: &'a str,
    pub file_size: i64,
    pub mime_type: &'a str,
    pub kind: &'a str,
    pub imported_at: i64,
    pub source: Option<ReferenceSource<'a>>,
}

pub struct ReferenceSource<'a> {
    pub path: &'a str,
    pub modified_at: i64,
}

pub fn insert_asset(conn: &Connection, asset: NewAsset<'_>) -> AppResult<i64> {
    let uid = asset
        .uid
        .map(str::to_owned)
        .unwrap_or_else(crate::util::new_entity_uid);

    let source_path = asset.source.as_ref().map(|source| source.path);
    let source_mtime = asset.source.as_ref().map(|source| source.modified_at);
    conn.execute(
        "INSERT INTO assets(uid,folder_id,file_name,rel_path,file_hash,file_size,mime_type,kind,imported_at,import_stage,source_path,source_mtime) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',?10,?11)",
        params![uid,asset.folder_id,asset.file_name,asset.rel_path,asset.file_hash,asset.file_size,asset.mime_type,asset.kind,asset.imported_at,source_path,source_mtime],
    )?;

    Ok(conn.last_insert_rowid())
}

/// Reimport relinks missing references to the new source path and resets processing state.
pub fn relink_reference(
    conn: &Connection,
    id: i64,
    source_path: &str,
    source_mtime: i64,
) -> AppResult<()> {
    let _size: i64 = conn.query_row(
        "SELECT MAX(file_size,0) FROM assets WHERE id=?1",
        [id],
        |row| row.get(0),
    )?;

    conn.execute(
        "UPDATE assets SET source_path = ?1, source_mtime = ?2, source_missing = 0, \
         import_stage = 'pending', deleted_at = NULL WHERE id = ?3",
        params![source_path, source_mtime, id],
    )?;

    Ok(())
}

/// List reference asset IDs and source paths for batch validation.
pub fn reference_assets(conn: &Connection) -> AppResult<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, source_path FROM assets
         WHERE source_path IS NOT NULL AND deleted_at IS NULL ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Include trashed reference paths when restoring asset:// authorization because they can
/// still be previewed or restored.
pub fn reference_source_paths(conn: &Connection) -> AppResult<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT source_path FROM assets
         WHERE source_path IS NOT NULL AND source_path <> '' ORDER BY source_path",
    )?;
    let rows = stmt
        .query_map([], |row| row.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Update the missing-source flag for batch checks and operation-time validation.
pub fn set_source_missing(conn: &Connection, id: i64, missing: bool) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET source_missing = ?1 WHERE id = ?2",
        params![missing, id],
    )?;
    Ok(())
}

/// Find a reference by its normalized source path.
pub fn find_reference_by_source(conn: &Connection, source_path: &str) -> AppResult<Option<i64>> {
    let mut stmt = conn
        .prepare("SELECT id FROM assets WHERE source_path = ?1 AND deleted_at IS NULL LIMIT 1")?;
    let mut rows = stmt.query_map(params![source_path], |row| row.get(0))?;
    match rows.next() {
        Some(row) => Ok(Some(row?)),
        None => Ok(None),
    }
}

/// Update the import stage.
pub fn set_import_stage(conn: &Connection, id: i64, stage: &str) -> AppResult<()> {
    super::processing::atomic(conn, || {
        conn.execute(
            "UPDATE assets SET import_stage=?1 WHERE id=?2",
            params![stage, id],
        )?;
        super::processing::import_stage(conn, id, stage)
    })
}

/// Recover media processing after startup. Assets awaiting embeddings already have derived
/// inputs and need only vector reconciliation.
pub fn recoverable_imports(conn: &Connection) -> AppResult<Vec<(i64, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, file_name FROM assets
         WHERE deleted_at IS NULL
           AND import_stage IN ('pending','copied','probing','thumb','frames','reindex')
         ORDER BY imported_at, id",
    )?;
    let rows = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Metadata produced by media probing and thumbnail generation.
#[derive(Default)]
pub struct AssetProbe<'a> {
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    pub color: Option<&'a str>,
    pub thumb_rel: Option<&'a str>,
}

pub fn update_asset_probe(conn: &Connection, id: i64, probe: AssetProbe<'_>) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET width=?1,height=?2,duration_ms=?3,color=?4,thumb_rel=?5 WHERE id=?6",
        params![
            probe.width,
            probe.height,
            probe.duration_ms,
            probe.color,
            probe.thumb_rel,
            id
        ],
    )?;
    Ok(())
}

/// Find an existing asset ID by file hash for deduplication.
pub fn find_by_hash(conn: &Connection, file_hash: &str) -> AppResult<Option<i64>> {
    let mut stmt = conn.prepare_cached("SELECT id FROM assets WHERE file_hash = ?1")?;
    Ok(stmt.query_row(params![file_hash], |r| r.get(0)).ok())
}

pub fn find_by_uid(conn: &Connection, uid: &str) -> AppResult<Option<(i64, String)>> {
    let mut stmt = conn.prepare_cached("SELECT id, file_hash FROM assets WHERE uid=?1")?;
    Ok(stmt.query_row([uid], |r| Ok((r.get(0)?, r.get(1)?))).ok())
}

/// Update folder membership after relocation.
pub fn set_asset_folder(conn: &Connection, id: i64, folder_id: Option<i64>) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET folder_id = ?1 WHERE id = ?2",
        params![folder_id, id],
    )?;
    Ok(())
}

/// Update a relative asset path after moving or renaming it.
pub fn set_asset_rel_path(
    conn: &Connection,
    id: i64,
    rel_path: &str,
    file_name: &str,
) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET rel_path = ?1, file_name = ?2 WHERE id = ?3",
        params![rel_path, file_name, id],
    )?;
    Ok(())
}

/// Soft-delete or restore an asset.
pub fn set_deleted(conn: &Connection, ids: &[i64], deleted_at: Option<i64>) -> AppResult<()> {
    let now = deleted_at;
    for &id in ids {
        if deleted_at.is_none() {
            use rusqlite::OptionalExtension;
            let _size = conn
                .query_row(
                    "SELECT MAX(file_size,0) FROM assets WHERE id=?1",
                    [id],
                    |row| row.get::<_, i64>(0),
                )
                .optional()?
                .unwrap_or(0);
        }
        conn.execute(
            "UPDATE assets SET deleted_at = ?1 WHERE id = ?2",
            params![now, id],
        )?;
    }
    Ok(())
}

/// Delete the asset row; foreign keys cascade to frames and tag associations.
pub fn delete_assets(conn: &Connection, ids: &[i64]) -> AppResult<()> {
    for &id in ids {
        conn.execute("DELETE FROM assets WHERE id = ?1", params![id])?;
    }
    Ok(())
}

pub mod asset_cleanup;
mod frames;
mod queries;

pub use frames::*;
pub(crate) use queries::build_where;
pub use queries::{
    get_asset, get_asset_by_uid, get_asset_detail, get_asset_detail_by_uid, get_assets_by_ids,
    get_frame_resource_by_uid, query_assets,
};

#[cfg(test)]
#[path = "../../tests/unit/db_assets.rs"]
mod tests;
