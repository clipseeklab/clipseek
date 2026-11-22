//! Keyframe persistence, representative colors and embedding scans.

use crate::db::model::FrameInfo;
use crate::error::AppResult;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};

/// Frame ID, UID, sequence, timestamp and relative path retained during transfer.
pub type TransferFrame = (i64, String, i64, i64, Option<String>);

/// Get internal index-unit IDs for synchronized vector deletion.
pub fn get_frame_ids(conn: &Connection, asset_id: i64) -> AppResult<Vec<(i64, Option<String>)>> {
    let mut stmt = conn.prepare("SELECT id, rel_path FROM frames WHERE asset_id = ?1")?;
    let out = stmt
        .query_map(params![asset_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Read one index unit and resolve its file path against the current library root.
pub fn get_frame(conn: &Connection, root: &Path, frame_id: i64) -> AppResult<Option<FrameInfo>> {
    let mut stmt =
        conn.prepare("SELECT id, seq, timestamp_ms, rel_path, uid FROM frames WHERE id = ?1")?;
    let mut rows = stmt.query(params![frame_id])?;
    let Some(r) = rows.next()? else {
        return Ok(None);
    };
    Ok(Some(FrameInfo {
        id: r.get(0)?,
        uid: r.get(4)?,
        seq: r.get(1)?,
        timestamp_ms: r.get(2)?,
        abs_path: r
            .get::<_, Option<String>>(3)?
            .map(|p| root.join(p).to_string_lossy().into_owned()),
    }))
}

/// Read a bounded candidate set without one statement per search hit.
pub fn get_frames_by_ids(conn: &Connection, root: &Path, ids: &[i64]) -> AppResult<Vec<FrameInfo>> {
    let mut frames = Vec::new();
    for chunk in ids.chunks(128) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let mut stmt = conn.prepare(&format!(
            "SELECT id,seq,timestamp_ms,rel_path,uid FROM frames WHERE id IN ({placeholders})"
        ))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |row| {
            Ok(FrameInfo {
                id: row.get(0)?,
                seq: row.get(1)?,
                timestamp_ms: row.get(2)?,
                uid: row.get(4)?,
                abs_path: row
                    .get::<_, Option<String>>(3)?
                    .map(|path| root.join(path).to_string_lossy().into_owned()),
            })
        })?;
        frames.extend(rows.collect::<Result<Vec<_>, _>>()?);
    }
    Ok(frames)
}

/// Preserve index-unit order, timestamps, and paths during transfer.
pub fn get_frames_for_transfer(conn: &Connection, asset_id: i64) -> AppResult<Vec<TransferFrame>> {
    let mut stmt = conn.prepare(
        "SELECT id, uid, seq, timestamp_ms, rel_path FROM frames WHERE asset_id = ?1 ORDER BY seq",
    )?;
    let frames = stmt
        .query_map(params![asset_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(frames)
}

/// Insert an internal index unit and return its frame/vector point ID.
pub fn insert_frame(
    conn: &Connection,
    asset_id: i64,
    seq: i64,
    timestamp_ms: i64,
    rel_path: Option<&str>,
) -> AppResult<i64> {
    insert_frame_with_uid(
        conn,
        &crate::util::new_entity_uid(),
        asset_id,
        seq,
        timestamp_ms,
        rel_path,
    )
}

pub fn insert_frame_with_uid(
    conn: &Connection,
    uid: &str,
    asset_id: i64,
    seq: i64,
    timestamp_ms: i64,
    rel_path: Option<&str>,
) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO frames(uid, asset_id, seq, timestamp_ms, rel_path) VALUES(?1, ?2, ?3, ?4, ?5)",
        params![uid, asset_id, seq, timestamp_ms, rel_path],
    )?;
    Ok(conn.last_insert_rowid())
}

/// Replace representative cover colors in rank order; callers supply at most three.
pub fn replace_asset_colors(
    conn: &Connection,
    asset_id: i64,
    colors: &[(&str, f32)],
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM asset_colors WHERE asset_id = ?1",
        params![asset_id],
    )?;
    for (rank, (color, ratio)) in colors.iter().enumerate() {
        conn.execute(
            "INSERT INTO asset_colors(asset_id, color, ratio, rank) VALUES(?1, ?2, ?3, ?4)",
            params![asset_id, color, ratio, rank as i64],
        )?;
    }
    Ok(())
}

pub fn set_asset_primary_color(conn: &Connection, asset_id: i64, color: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET color = ?1 WHERE id = ?2",
        params![color, asset_id],
    )?;
    Ok(())
}

/// Replace representative colors for one search unit.
pub fn replace_frame_colors(
    conn: &Connection,
    frame_id: i64,
    colors: &[(&str, f32)],
) -> AppResult<()> {
    conn.execute(
        "DELETE FROM frame_colors WHERE frame_id = ?1",
        params![frame_id],
    )?;
    for (rank, (color, ratio)) in colors.iter().enumerate() {
        conn.execute(
            "INSERT INTO frame_colors(frame_id, color, ratio, rank) VALUES(?1, ?2, ?3, ?4)",
            params![frame_id, color, ratio, rank as i64],
        )?;
    }
    Ok(())
}

pub fn frame_colors(conn: &Connection, frame_id: i64) -> AppResult<Vec<String>> {
    let mut stmt =
        conn.prepare("SELECT color FROM frame_colors WHERE frame_id = ?1 ORDER BY rank")?;
    let colors = stmt
        .query_map(params![frame_id], |r| r.get(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(colors)
}

/// Scan pending embeddings, returning frame ID, asset ID, and absolute input path.
pub fn pending_frames(
    conn: &Connection,
    root: &Path,
    limit: i64,
) -> AppResult<Vec<(i64, i64, PathBuf)>> {
    let mut stmt = conn.prepare(
        "SELECT fr.id, fr.asset_id, COALESCE(fr.rel_path, a.thumb_rel, a.rel_path) \
         FROM frames fr JOIN assets a ON a.id = fr.asset_id \
         WHERE fr.embedded = 0 AND a.deleted_at IS NULL AND (a.visual_requested=1 OR EXISTS(SELECT 1 FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' AND it.status IN ('queued','running'))) AND COALESCE((SELECT it.status FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' ORDER BY it.id DESC LIMIT 1),'queued') IN ('queued','running','completed') AND (a.visual_indexed=1 OR COALESCE((SELECT it.status FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' ORDER BY it.id DESC LIMIT 1),'queued')<>'completed') LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            let rel: String = r.get(2)?;
            Ok((r.get(0)?, r.get(1)?, root.join(rel)))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Mark an index unit as embedded.
pub fn mark_frame_embedded(conn: &Connection, frame_id: i64) -> AppResult<()> {
    conn.execute(
        "UPDATE frames SET embedded = 1 WHERE id = ?1",
        params![frame_id],
    )?;
    conn.execute("UPDATE assets SET visual_indexed=1 WHERE id=(SELECT asset_id FROM frames WHERE id=?1) AND EXISTS(SELECT 1 FROM frames f WHERE f.asset_id=assets.id) AND NOT EXISTS(SELECT 1 FROM frames f WHERE f.asset_id=assets.id AND f.embedded=0) AND COALESCE((SELECT t.status FROM index_tasks t WHERE t.asset_id=assets.id AND t.kind='visual' ORDER BY t.id DESC LIMIT 1),'queued') NOT IN ('running','cancelled','failed')",[frame_id])?;
    Ok(())
}

/// Count pending units for embedding progress.
pub fn count_pending_frames(conn: &Connection) -> AppResult<i64> {
    let count = conn.query_row("SELECT COUNT(*) FROM frames fr JOIN assets a ON a.id=fr.asset_id WHERE fr.embedded = 0 AND a.deleted_at IS NULL AND (a.visual_requested=1 OR EXISTS(SELECT 1 FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' AND it.status IN ('queued','running'))) AND COALESCE((SELECT it.status FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' ORDER BY it.id DESC LIMIT 1),'queued') IN ('queued','running','completed') AND (a.visual_indexed=1 OR COALESCE((SELECT it.status FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' ORDER BY it.id DESC LIMIT 1),'queued')<>'completed')", [], |r| {
        r.get(0)
    })?;
    Ok(count)
}

/// Fetch pending units with asset fields required for vector payloads.
pub struct PendingFrame {
    pub frame_id: i64,
    pub asset_id: i64,
    pub seq: i64,
    pub timestamp_ms: i64,
    pub path: PathBuf,
    pub folder_id: Option<i64>,
    pub kind: String,
    pub colors: Vec<String>,
    pub tags: Vec<String>,
    pub deleted: bool,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub size: i64,
    pub imported_at: i64,
}

/// Scan pending units with asset fields and aggregated tags.
pub fn pending_frames_full(
    conn: &Connection,
    root: &Path,
    limit: i64,
) -> AppResult<Vec<PendingFrame>> {
    let mut stmt = conn.prepare(
        "SELECT fr.id, fr.asset_id, fr.seq, fr.timestamp_ms, COALESCE(fr.rel_path, a.thumb_rel, a.rel_path), \
                a.folder_id, a.kind, a.file_size, a.imported_at, \
                CASE WHEN a.deleted_at IS NULL THEN 0 ELSE 1 END, a.width, a.height, \
                (SELECT GROUP_CONCAT(t.name, ',') FROM asset_tags at2 JOIN tags t ON t.id = at2.tag_id WHERE at2.asset_id = a.id), \
                (SELECT GROUP_CONCAT(fc.color, ',') FROM frame_colors fc WHERE fc.frame_id = fr.id ORDER BY fc.rank) \
         FROM frames fr JOIN assets a ON a.id = fr.asset_id \
         WHERE fr.embedded = 0 AND a.deleted_at IS NULL AND (a.visual_requested=1 OR EXISTS(SELECT 1 FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' AND it.status IN ('queued','running'))) AND COALESCE((SELECT it.status FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' ORDER BY it.id DESC LIMIT 1),'queued') IN ('queued','running','completed') AND (a.visual_indexed=1 OR COALESCE((SELECT it.status FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual' ORDER BY it.id DESC LIMIT 1),'queued')<>'completed') LIMIT ?1",
    )?;
    let rows = stmt
        .query_map(params![limit], |r| {
            let rel: String = r.get(4)?;
            Ok(PendingFrame {
                frame_id: r.get(0)?,
                asset_id: r.get(1)?,
                seq: r.get(2)?,
                timestamp_ms: r.get(3)?,
                path: root.join(rel),
                folder_id: r.get(5)?,
                kind: r.get(6)?,
                size: r.get(7)?,
                imported_at: r.get(8)?,
                deleted: r.get::<_, i64>(9)? != 0,
                width: r.get(10)?,
                height: r.get(11)?,
                tags: r
                    .get::<_, Option<String>>(12)?
                    .filter(|s| !s.is_empty())
                    .map(|s| s.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
                colors: r
                    .get::<_, Option<String>>(13)?
                    .filter(|s| !s.is_empty())
                    .map(|s| s.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Read all index units by frame-ID cursor for a full rebuild, regardless of embedded state.
pub fn frames_full_after(
    conn: &Connection,
    root: &Path,
    after_id: i64,
    limit: i64,
) -> AppResult<Vec<PendingFrame>> {
    let mut stmt = conn.prepare(
        "SELECT fr.id, fr.asset_id, fr.seq, fr.timestamp_ms, COALESCE(fr.rel_path, a.thumb_rel, a.rel_path), \
                a.folder_id, a.kind, a.file_size, a.imported_at, \
                CASE WHEN a.deleted_at IS NULL THEN 0 ELSE 1 END, a.width, a.height, \
                (SELECT GROUP_CONCAT(t.name, ',') FROM asset_tags at2 JOIN tags t ON t.id = at2.tag_id WHERE at2.asset_id = a.id), \
                (SELECT GROUP_CONCAT(fc.color, ',') FROM frame_colors fc WHERE fc.frame_id = fr.id ORDER BY fc.rank) \
         FROM frames fr JOIN assets a ON a.id = fr.asset_id \
         WHERE fr.id > ?1 AND (a.visual_indexed=1 OR (a.visual_requested=1 AND NOT EXISTS(SELECT 1 FROM index_tasks it WHERE it.asset_id=a.id AND it.kind='visual'))) ORDER BY fr.id LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![after_id, limit], |r| {
            let rel: String = r.get(4)?;
            Ok(PendingFrame {
                frame_id: r.get(0)?,
                asset_id: r.get(1)?,
                seq: r.get(2)?,
                timestamp_ms: r.get(3)?,
                path: root.join(rel),
                folder_id: r.get(5)?,
                kind: r.get(6)?,
                size: r.get(7)?,
                imported_at: r.get(8)?,
                deleted: r.get::<_, i64>(9)? != 0,
                width: r.get(10)?,
                height: r.get(11)?,
                tags: r
                    .get::<_, Option<String>>(12)?
                    .filter(|s| !s.is_empty())
                    .map(|s| s.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
                colors: r
                    .get::<_, Option<String>>(13)?
                    .filter(|s| !s.is_empty())
                    .map(|s| s.split(',').map(str::to_string).collect())
                    .unwrap_or_default(),
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
