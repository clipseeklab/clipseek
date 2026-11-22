//! SQLite candidate exclusions for semantic search, evaluated before top-k recall.

use rusqlite::Connection;

use crate::error::AppResult;

/// Unready visual generations are never searchable. Subtitle-state filtering
/// applies only to videos; requesting unindexed visuals excludes ready ones too.
pub(crate) fn excluded_visual_frame_ids(
    conn: &Connection,
    visual_indexed: Option<bool>,
    subtitle_indexed: Option<bool>,
) -> AppResult<Vec<i64>> {
    let mut sql =
        "SELECT fr.id FROM frames fr JOIN assets a ON a.id=fr.asset_id WHERE a.visual_indexed=0"
            .to_owned();
    if let Some(value) = subtitle_indexed {
        sql.push_str(" OR a.kind<>'video'");
        sql.push_str(if value {
            " OR a.subtitle_indexed=0"
        } else {
            " OR a.subtitle_indexed=1"
        });
    }
    if visual_indexed == Some(false) {
        sql.push_str(" OR a.visual_indexed=1");
    }
    let mut statement = conn.prepare(&sql)?;
    let ids = statement
        .query_map([], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids)
}

#[cfg(test)]
#[path = "../../tests/unit/db_search.rs"]
mod tests;
