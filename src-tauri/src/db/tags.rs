use rusqlite::{params, Connection};

use super::model::Tag;
use crate::error::AppResult;

/// List tags by name with live asset counts.
pub fn list(conn: &Connection) -> AppResult<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, COUNT(a.id), t.created_at \
         FROM tags t \
         LEFT JOIN asset_tags at ON at.tag_id = t.id \
         LEFT JOIN assets a ON a.id = at.asset_id AND a.deleted_at IS NULL \
         GROUP BY t.id ORDER BY t.name COLLATE NOCASE ASC",
    )?;
    let out = stmt
        .query_map([], |r| {
            Ok(Tag {
                id: r.get(0)?,
                name: r.get(1)?,
                asset_count: r.get(2)?,
                created_at: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Add tags by name, deduplicate, and create missing tags. Commas are forbidden because
/// listing uses GROUP_CONCAT.
pub fn add_to_asset(conn: &Connection, asset_id: i64, names: &[String]) -> AppResult<()> {
    let now = crate::util::now();
    for raw in names {
        let name = raw.trim();
        if name.is_empty() || name.contains(',') {
            continue;
        }
        conn.execute(
            "INSERT INTO tags(name, created_at) VALUES(?1, ?2) ON CONFLICT(name) DO NOTHING",
            params![name, now],
        )?;
        conn.execute(
            "INSERT INTO asset_tags(asset_id, tag_id) \
             SELECT ?1, id FROM tags WHERE name = ?2 \
             ON CONFLICT DO NOTHING",
            params![asset_id, name],
        )?;
    }
    Ok(())
}

/// Remove one tag association.
pub fn remove_from_asset(conn: &Connection, asset_id: i64, tag_id: i64) -> AppResult<()> {
    conn.execute(
        "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
        params![asset_id, tag_id],
    )?;
    Ok(())
}

/// Rename a tag globally.
pub fn rename(conn: &Connection, id: i64, new_name: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE tags SET name = ?1 WHERE id = ?2",
        params![new_name, id],
    )?;
    Ok(())
}

/// Delete a tag and cascade its associations.
pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
    Ok(())
}
