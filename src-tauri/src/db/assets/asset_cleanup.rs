//! Durable deletion intent. Asset rows retain paths and frame IDs until cleanup succeeds.
use rusqlite::Connection;

use crate::error::{AppError, AppResult};

const PREFIX: &str = "asset_delete_pending.v1.";

pub fn pending(conn: &Connection, uid: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM meta WHERE key=?1)",
        [format!("{PREFIX}{uid}")],
        |row| row.get(0),
    )?)
}

pub fn mark(conn: &Connection, uid: &str) -> AppResult<()> {
    conn.execute(
        "INSERT OR IGNORE INTO meta(key,value) VALUES(?1,'pending')",
        [format!("{PREFIX}{uid}")],
    )?;
    // A failed filesystem validation can leave old vector payloads untouched.
    // Invalidate readiness in SQLite so search excludes the retained tombstone.
    conn.execute(
        "UPDATE assets SET visual_indexed=0,subtitle_indexed=0 WHERE uid=?1",
        [uid],
    )?;
    Ok(())
}

pub fn clear(conn: &Connection, uid: &str) -> AppResult<()> {
    conn.execute("DELETE FROM meta WHERE key=?1", [format!("{PREFIX}{uid}")])?;
    Ok(())
}

pub fn ensure_not_pending(conn: &Connection, id: i64) -> AppResult<()> {
    let blocked: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM assets a JOIN meta m ON m.key=?1 || a.uid WHERE a.id=?2)",
        rusqlite::params![PREFIX, id],
        |row| row.get(0),
    )?;
    if blocked {
        return Err(AppError::coded("asset_delete_pending", "Permanent deletion has started; retry deletion instead of restoring or reusing the asset"));
    }
    Ok(())
}

pub fn expired_or_pending(conn: &Connection, cutoff: i64) -> AppResult<Vec<i64>> {
    let mut stmt = conn.prepare(
        "SELECT a.id FROM assets a WHERE a.deleted_at IS NOT NULL AND
         (a.deleted_at < ?1 OR EXISTS(SELECT 1 FROM meta m WHERE m.key=?2 || a.uid)) ORDER BY a.id",
    )?;
    let rows = stmt.query_map(rusqlite::params![cutoff, PREFIX], |row| row.get(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}
