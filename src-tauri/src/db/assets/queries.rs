//! Asset reads, filter SQL and the shared row mapping.

use crate::db::model::{Asset, AssetDetail, AssetFilter, FrameInfo, Paged};
use crate::error::AppResult;
use rusqlite::types::Value as SqlValue;
use rusqlite::{params, Connection, Row};
use std::path::Path;

/// Shared asset SELECT, including folder name and aggregated tags.
const ASSET_COLS: &str = "a.id, a.folder_id, f.name, a.file_name, a.rel_path, a.file_size, \
     a.mime_type, a.kind, a.width, a.height, a.duration_ms, a.color, a.thumb_rel, \
     a.imported_at, a.deleted_at, a.import_stage, \
     (SELECT GROUP_CONCAT(t.name, ',') FROM asset_tags at JOIN tags t ON t.id = at.tag_id WHERE at.asset_id = a.id), a.uid, \
     a.source_path, a.source_mtime, a.source_missing, a.web_source_url, a.web_source_title, a.display_name";

// Read-only source libraries may predate independent index state.
fn asset_columns(conn: &Connection) -> String {
    let online = if crate::db::indexing::has_column(conn, "assets", "online_source") {
        "a.online_source"
    } else {
        "NULL"
    };
    let processing = if crate::db::indexing::has_column(conn, "processing_tasks", "id") {
        "(SELECT COALESCE(json_group_array(json_object('id',p.id,'kind',p.kind,'status',p.status,'phase',p.phase,'error',p.error,'attempt',p.attempt,'indexTaskId',p.index_task_id,'createdAt',p.created_at,'updatedAt',p.updated_at)),'[]') FROM processing_tasks p WHERE p.asset_id=a.id )"
    } else {
        "'[]'"
    };
    if crate::db::indexing::has_column(conn, "assets", "visual_indexed") {
        format!("{ASSET_COLS},a.visual_indexed,a.subtitle_indexed,EXISTS(SELECT 1 FROM subtitle_sources ss WHERE ss.asset_id=a.id),{processing},{online}")
    } else {
        format!("{ASSET_COLS},EXISTS(SELECT 1 FROM frames v WHERE v.asset_id=a.id) AND NOT EXISTS(SELECT 1 FROM frames v WHERE v.asset_id=a.id AND v.embedded=0),EXISTS(SELECT 1 FROM subtitle_tracks t JOIN subtitle_docs d ON d.asset_id=t.asset_id AND d.version=t.version WHERE t.asset_id=a.id),EXISTS(SELECT 1 FROM subtitle_tracks st WHERE st.asset_id=a.id),{processing},{online}")
    }
}

/// Map a row to Asset using root for absolute paths. Column order must match ASSET_COLS:
/// thumb_rel=12, imported_at=13, deleted_at=14, import_stage=15, tags=16, source_path=18,
/// source_mtime=19, source_missing=20, web_source_url=21, web_source_title=22,
/// display_name=23.
fn row_to_asset(row: &Row, root: &Path) -> rusqlite::Result<Asset> {
    let rel_path: String = row.get(4)?;
    let thumb_rel: Option<String> = row.get(12)?;
    let source_path: Option<String> = row.get(18)?;
    let source_mtime: Option<i64> = row.get(19)?;
    let join = |rel: &str| root.join(rel).to_string_lossy().into_owned();
    // Reference abs_path points to the original source; legacy managed assets point inside
    // the library.
    let (storage, abs_path) = match source_path.as_deref() {
        Some(source) => ("reference", source.to_string()),
        None => ("copy", join(&rel_path)),
    };
    Ok(Asset {
        id: row.get(0)?,
        uid: row.get(17)?,
        folder_id: row.get(1)?,
        folder_name: row.get(2)?,
        file_name: row.get(3)?,
        display_name: row.get(23)?,
        abs_path,
        storage,
        source_path,
        source_mtime,
        source_missing: row.get::<_, Option<i64>>(20)?.unwrap_or(0) != 0,
        web_source_url: row.get(21)?,
        web_source_title: row.get(22)?,
        rel_path,
        file_size: row.get(5)?,
        mime_type: row.get(6)?,
        kind: row.get(7)?,
        width: row.get(8)?,
        height: row.get(9)?,
        duration_ms: row.get(10)?,
        color: row.get(11)?,
        thumb_path: thumb_rel.as_deref().map(join),
        imported_at: row.get(13)?,
        deleted_at: row.get(14)?,
        import_stage: row.get(15)?,
        processing_tasks: serde_json::from_str(&row.get::<_, String>(27)?).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(27, rusqlite::types::Type::Text, Box::new(e))
        })?,
        visual_indexed: row.get(24)?,
        subtitle_indexed: row.get(25)?,
        subtitle_available: row.get(26)?,
        tags: row
            .get::<_, Option<String>>(16)?
            .filter(|s| !s.is_empty())
            .map(|s| s.split(',').map(str::to_string).collect())
            .unwrap_or_default(),
    })
}

/// Compile filters into ordered SQL placeholders and parameters.
pub(crate) fn build_where(f: &AssetFilter) -> (String, Vec<SqlValue>) {
    let mut clauses: Vec<String> = Vec::new();
    let mut pv: Vec<SqlValue> = Vec::new();

    // Normal views exclude deleted assets; trash views include only deleted assets.
    clauses.push(if f.in_trash {
        "a.deleted_at IS NOT NULL".into()
    } else {
        "a.deleted_at IS NULL".into()
    });

    if let Some(asset_id) = f.asset_id {
        clauses.push("a.id = ?".into());
        pv.push(SqlValue::from(asset_id));
    }

    if let Some(q) = f.query.as_deref().filter(|s| !s.trim().is_empty()) {
        clauses
            .push("(a.file_name LIKE ? ESCAPE '\\' OR a.display_name LIKE ? ESCAPE '\\')".into());
        let pattern = like_pattern(q);
        pv.push(SqlValue::from(pattern.clone()));
        pv.push(SqlValue::from(pattern));
    }
    if let Some(value) = f.visual_indexed {
        clauses.push("a.visual_indexed = ?".into());
        pv.push(SqlValue::Integer(value as i64));
    }
    if let Some(value) = f.subtitle_indexed {
        clauses.push("a.kind = 'video' AND a.subtitle_indexed = ?".into());
        pv.push(SqlValue::Integer(value as i64));
    }
    if f.failed_only {
        clauses.push("a.import_stage LIKE 'error%'".into());
    }
    if f.source_missing {
        clauses.push("a.source_missing = 1".into());
    }
    if let Some(fid) = f.folder_id {
        clauses.push("a.folder_id = ?".into());
        pv.push(SqlValue::from(fid));
    } else if f.uncategorized || !f.folder_ids.is_empty() {
        let mut folder_parts = Vec::new();
        if !f.folder_ids.is_empty() {
            let mut qs = Vec::new();
            for fid in &f.folder_ids {
                qs.push("?".to_string());
                pv.push(SqlValue::from(*fid));
            }
            folder_parts.push(format!("a.folder_id IN ({})", qs.join(",")));
        }
        if f.uncategorized {
            folder_parts.push("a.folder_id IS NULL".into());
        }
        clauses.push(format!("({})", folder_parts.join(" OR ")));
    }
    if !f.tags.is_empty() {
        // Require all selected tags by comparing matching and selected tag counts.
        let mut qs = Vec::new();
        for t in &f.tags {
            qs.push("?".to_string());
            pv.push(SqlValue::from(t.clone()));
        }
        clauses.push(format!(
            "a.id IN (SELECT at2.asset_id FROM tags t JOIN asset_tags at2 ON at2.tag_id = t.id \
             WHERE t.name IN ({}) GROUP BY at2.asset_id HAVING COUNT(DISTINCT t.name) = {})",
            qs.join(","),
            f.tags.len()
        ));
    }
    if !f.kinds.is_empty() {
        let mut qs = Vec::new();
        for k in &f.kinds {
            qs.push("?".to_string());
            pv.push(SqlValue::from(k.clone()));
        }
        clauses.push(format!("a.kind IN ({})", qs.join(",")));
    }
    if !f.colors.is_empty() {
        let mut qs = Vec::new();
        for c in &f.colors {
            qs.push("?".to_string());
            pv.push(SqlValue::from(c.clone()));
        }
        clauses.push(format!(
            "EXISTS (SELECT 1 FROM asset_colors ac WHERE ac.asset_id = a.id AND ac.color IN ({}))",
            qs.join(",")
        ));
    }
    if let Some(w) = f.min_width {
        clauses.push("a.width >= ?".into());
        pv.push(SqlValue::from(w));
    }
    if let Some(h) = f.min_height {
        clauses.push("a.height >= ?".into());
        pv.push(SqlValue::from(h));
    }
    if let Some(v) = f.min_aspect {
        clauses.push("(CAST(a.width AS REAL) / NULLIF(a.height, 0)) >= ?".into());
        pv.push(SqlValue::from(v));
    }
    if let Some(v) = f.max_aspect {
        clauses.push("(CAST(a.width AS REAL) / NULLIF(a.height, 0)) <= ?".into());
        pv.push(SqlValue::from(v));
    }
    if let Some(s) = f.min_size {
        clauses.push("a.file_size >= ?".into());
        pv.push(SqlValue::from(s));
    }
    if let Some(s) = f.max_size {
        clauses.push("a.file_size <= ?".into());
        pv.push(SqlValue::from(s));
    }
    if let Some(d) = f.date_from {
        clauses.push("a.imported_at >= ?".into());
        pv.push(SqlValue::from(d));
    }
    if let Some(d) = f.date_to {
        clauses.push("a.imported_at <= ?".into());
        pv.push(SqlValue::from(d));
    }

    (clauses.join(" AND "), pv)
}

/// Build a LIKE pattern with surrounding percent signs and escaped percent, underscore, and
/// backslash.
fn like_pattern(q: &str) -> String {
    let escaped = q
        .trim()
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_");
    format!("%{escaped}%")
}

fn order_sql(sort: Option<&str>) -> &'static str {
    match sort.unwrap_or("imported_desc") {
        "imported_asc" => "a.imported_at ASC, a.id ASC",
        "name_asc" => "a.file_name COLLATE NOCASE ASC",
        "name_desc" => "a.file_name COLLATE NOCASE DESC",
        "size_desc" => "a.file_size DESC",
        _ => "a.imported_at DESC, a.id DESC",
    }
}

/// Query a page of assets.
pub fn query_assets(conn: &Connection, root: &Path, f: &AssetFilter) -> AppResult<Paged<Asset>> {
    let (wh, pv) = build_where(f);
    let page = f.page.max(1);
    // Compute exact totals only on the first page; later pages use has_more to avoid repeated
    // scans.
    let total: i64 = if page == 1 {
        conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM assets a LEFT JOIN folders f ON f.id = a.folder_id WHERE {wh}"
            ),
            rusqlite::params_from_iter(pv.iter()),
            |r| r.get(0),
        )?
    } else {
        0
    };

    let page_size = f.page_size.clamp(1, 200) as i64;
    let default_sort = f.sort.as_deref().unwrap_or("imported_desc") == "imported_desc";
    let mut page_where = wh;
    let mut page_params = pv;
    let offset = if default_sort {
        if let (Some(imported_at), Some(id)) = (f.cursor_imported_at, f.cursor_id) {
            page_where.push_str(" AND (a.imported_at < ? OR (a.imported_at = ? AND a.id < ?))");
            page_params.push(SqlValue::from(imported_at));
            page_params.push(SqlValue::from(imported_at));
            page_params.push(SqlValue::from(id));
            0
        } else {
            (page as i64 - 1) * page_size
        }
    } else {
        (page as i64 - 1) * page_size
    };
    let fetch_limit = page_size + 1;
    let asset_cols = asset_columns(conn);
    let sql = format!(
        "SELECT {asset_cols} FROM assets a LEFT JOIN folders f ON f.id = a.folder_id \
         WHERE {page_where} ORDER BY {} LIMIT {fetch_limit} OFFSET {offset}",
        order_sql(f.sort.as_deref())
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut items = stmt
        .query_map(rusqlite::params_from_iter(page_params.iter()), |r| {
            row_to_asset(r, root)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let has_more = items.len() > page_size as usize;
    items.truncate(page_size as usize);
    Ok(Paged {
        items,
        total,
        page,
        page_size: page_size as u32,
        has_more,
    })
}

/// External APIs use stable cross-library UIDs rather than local autoincrement IDs.
pub fn get_asset_by_uid(conn: &Connection, root: &Path, uid: &str) -> AppResult<Option<Asset>> {
    let asset_cols = asset_columns(conn);
    let sql = format!(
        "SELECT {asset_cols} FROM assets a LEFT JOIN folders f ON f.id = a.folder_id \
         WHERE a.uid = ?1 LIMIT 1"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query([uid])?;
    Ok(match rows.next()? {
        Some(row) => Some(row_to_asset(row, root)?),
        None => None,
    })
}

/// Read asset details by stable UID.
pub fn get_asset_detail_by_uid(
    conn: &Connection,
    root: &Path,
    uid: &str,
) -> AppResult<Option<AssetDetail>> {
    let Some(asset) = get_asset_by_uid(conn, root, uid)? else {
        return Ok(None);
    };
    get_asset_detail(conn, root, asset.id)
}

/// Resolve resource paths and owning asset state by stable index-unit UID.
pub fn get_frame_resource_by_uid(
    conn: &Connection,
    root: &Path,
    uid: &str,
) -> AppResult<Option<(FrameInfo, bool)>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.uid, f.seq, f.timestamp_ms, f.rel_path, a.thumb_rel, \
                a.deleted_at IS NOT NULL \
         FROM frames f JOIN assets a ON a.id = f.asset_id WHERE f.uid = ?1 LIMIT 1",
    )?;
    let mut rows = stmt.query([uid])?;
    let Some(row) = rows.next()? else {
        return Ok(None);
    };
    let frame_rel: Option<String> = row.get(4)?;
    let thumb_rel: Option<String> = row.get(5)?;
    let rel = frame_rel.or(thumb_rel);
    Ok(Some((
        FrameInfo {
            id: row.get(0)?,
            uid: row.get(1)?,
            seq: row.get(2)?,
            timestamp_ms: row.get(3)?,
            abs_path: rel.map(|path| root.join(path).to_string_lossy().into_owned()),
        },
        row.get(6)?,
    )))
}

/// Read one asset, returning None when absent.
pub fn get_asset(conn: &Connection, root: &Path, id: i64) -> AppResult<Option<Asset>> {
    let asset_cols = asset_columns(conn);
    let sql = format!("SELECT {asset_cols} FROM assets a LEFT JOIN folders f ON f.id = a.folder_id WHERE a.id = ?");
    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query(params![id])?;
    if let Some(r) = rows.next()? {
        return Ok(Some(row_to_asset(r, root)?));
    }
    Ok(None)
}

/// Read IDs in request order, skipping missing records.
pub fn get_assets_by_ids(conn: &Connection, root: &Path, ids: &[i64]) -> AppResult<Vec<Asset>> {
    let columns = asset_columns(conn);
    let mut found = std::collections::HashMap::new();
    for chunk in ids.chunks(128) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!("SELECT {columns} FROM assets a LEFT JOIN folders f ON f.id=a.folder_id WHERE a.id IN ({placeholders})");
        let mut stmt = conn.prepare(&sql)?;
        for asset in stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            row_to_asset(row, root)
        })? {
            let asset = asset?;
            found.insert(asset.id, asset);
        }
    }
    Ok(ids.iter().filter_map(|id| found.get(id).cloned()).collect())
}

/// Asset details with internal index units.
pub fn get_asset_detail(conn: &Connection, root: &Path, id: i64) -> AppResult<Option<AssetDetail>> {
    let Some(asset) = get_asset(conn, root, id)? else {
        return Ok(None);
    };
    let mut stmt = conn.prepare(
        "SELECT id, seq, timestamp_ms, rel_path, uid FROM frames WHERE asset_id = ? ORDER BY seq ASC",
    )?;
    let frames = stmt
        .query_map(params![id], |r| {
            Ok(FrameInfo {
                id: r.get(0)?,
                uid: r.get(4)?,
                seq: r.get(1)?,
                timestamp_ms: r.get(2)?,
                abs_path: r
                    .get::<_, Option<String>>(3)?
                    .map(|p| root.join(p).to_string_lossy().into_owned()),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(AssetDetail { asset, frames }))
}
