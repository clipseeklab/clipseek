use rusqlite::{params, Connection, OptionalExtension};

use super::model::Folder;
use crate::error::{AppError, AppResult};

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderPage {
    pub items: Vec<Folder>,
    pub total: i64,
    pub page: i64,
    pub has_more: bool,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSidebar {
    pub items: Vec<Folder>,
    pub total: i64,
    pub display_limit: i64,
}
const PIN_KEY: &str = "folder_sidebar_ids.v1";
const DISPLAY_LIMIT_KEY: &str = "folder_sidebar_display_limit.v1";

fn display_limit(conn: &Connection) -> AppResult<i64> {
    let saved: Option<String> = conn
        .query_row(
            "SELECT value FROM meta WHERE key=?",
            [DISPLAY_LIMIT_KEY],
            |r| r.get(0),
        )
        .optional()?;
    Ok(saved
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| (5..=30).contains(value))
        .unwrap_or(10))
}

pub fn page(conn: &Connection, query: &str, page: i64) -> AppResult<FolderPage> {
    page_sorted(conn, query, page, "created_desc")
}

pub fn page_sorted(conn: &Connection, query: &str, page: i64, sort: &str) -> AppResult<FolderPage> {
    let (order, outer_order) = match sort {
        "name_asc" => (
            "name COLLATE NOCASE ASC,id ASC",
            "f.name COLLATE NOCASE ASC,f.id ASC",
        ),
        "name_desc" => (
            "name COLLATE NOCASE DESC,id DESC",
            "f.name COLLATE NOCASE DESC,f.id DESC",
        ),
        "created_asc" => ("created_at ASC,id ASC", "f.created_at ASC,f.id ASC"),
        "created_desc" => ("created_at DESC,id DESC", "f.created_at DESC,f.id DESC"),
        _ => return Err(AppError::InvalidArgument("Invalid folder sort".into())),
    };
    if !(1..=i64::MAX / 40).contains(&page) || query.len() > 4000 {
        return Err(AppError::InvalidArgument("Invalid folder page".into()));
    }
    let total = conn.query_row(
        "SELECT count(*) FROM folders WHERE instr(lower(name),lower(?))>0",
        [query.trim()],
        |r| r.get::<_, i64>(0),
    )?;
    // ORDER BY fragments are selected only from the fixed allowlist above.
    let sql = format!("SELECT f.id,f.name,f.created_at,count(a.id) FROM (SELECT * FROM folders WHERE instr(lower(name),lower(?1))>0 ORDER BY {order} LIMIT 40 OFFSET ?2) f LEFT JOIN assets a ON a.folder_id=f.id AND a.deleted_at IS NULL GROUP BY f.id ORDER BY {outer_order}");
    let mut stmt = conn.prepare(&sql)?;
    let items = stmt
        .query_map(params![query.trim(), (page - 1) * 40], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                asset_count: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(FolderPage {
        items,
        total,
        page,
        has_more: page * 40 < total,
    })
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Option<Folder>> {
    Ok(conn.query_row("SELECT f.id,f.name,f.created_at,count(a.id) FROM folders f LEFT JOIN assets a ON a.folder_id=f.id AND a.deleted_at IS NULL WHERE f.id=? GROUP BY f.id",[id],|r|Ok(Folder{id:r.get(0)?,name:r.get(1)?,created_at:r.get(2)?,asset_count:r.get(3)?})).optional()?)
}

fn pin_ids(conn: &Connection) -> AppResult<Vec<i64>> {
    let saved: Option<String> = conn
        .query_row("SELECT value FROM meta WHERE key=?", [PIN_KEY], |r| {
            r.get(0)
        })
        .optional()?;
    let ids = if let Some(saved) = saved {
        serde_json::from_str::<Vec<i64>>(&saved)?
    } else {
        let mut stmt =
            conn.prepare("SELECT id FROM folders ORDER BY name COLLATE NOCASE,id LIMIT 10")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<i64>, _>>()?;
        conn.execute(
            "INSERT INTO meta(key,value) VALUES(?1,?2)",
            params![PIN_KEY, serde_json::to_string(&ids)?],
        )?;
        ids
    };
    let mut result = Vec::new();
    for id in ids {
        if result.len() < 30 && !result.contains(&id) && get_name(conn, id)?.is_some() {
            result.push(id);
        }
    }
    Ok(result)
}

pub fn sidebar(conn: &Connection) -> AppResult<FolderSidebar> {
    let ids = pin_ids(conn)?;
    let items = ids
        .into_iter()
        .map(|id| get(conn, id))
        .collect::<AppResult<Vec<_>>>()?
        .into_iter()
        .flatten()
        .collect();
    Ok(FolderSidebar {
        items,
        total: conn.query_row("SELECT count(*) FROM folders", [], |r| r.get(0))?,
        display_limit: display_limit(conn)?,
    })
}

pub fn set_sidebar(conn: &Connection, ids: &[i64]) -> AppResult<FolderSidebar> {
    set_sidebar_with_limit(conn, ids, None)
}

pub fn set_sidebar_with_limit(
    conn: &Connection,
    ids: &[i64],
    limit: Option<i64>,
) -> AppResult<FolderSidebar> {
    if limit.is_some_and(|value| !(5..=30).contains(&value)) {
        return Err(AppError::InvalidArgument(
            "Display limit must be between 5 and 30".into(),
        ));
    }
    if ids.len() > 30 {
        return Err(AppError::coded(
            "folder_pin_limit",
            "At most 30 folders can be pinned",
        ));
    }
    let unique = ids.iter().collect::<std::collections::HashSet<_>>();
    if unique.len() != ids.len() {
        return Err(AppError::InvalidArgument("Duplicate pinned folders".into()));
    }
    for id in ids {
        if get_name(conn, *id)?.is_none() {
            return Err(AppError::coded("folder_not_found", "Folder not found"));
        }
    }
    conn.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![PIN_KEY,serde_json::to_string(ids)?])?;
    if let Some(limit) = limit {
        conn.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",params![DISPLAY_LIMIT_KEY,limit.to_string()])?;
    }
    sidebar(conn)
}

pub fn pin(conn: &Connection, id: i64, pinned: bool) -> AppResult<FolderSidebar> {
    let mut ids = pin_ids(conn)?;
    ids.retain(|value| *value != id);
    if pinned {
        ids.insert(0, id);
        ids.truncate(30);
    }
    set_sidebar(conn, &ids)
}

fn folder_write_error(error: rusqlite::Error) -> AppError {
    if matches!(&error, rusqlite::Error::SqliteFailure(code, _) if code.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE)
    {
        AppError::coded(
            "folder_duplicate",
            "A folder with this name already exists. Choose a different name.",
        )
    } else {
        error.into()
    }
}

/// List folders by name with live asset counts.
pub fn list(conn: &Connection) -> AppResult<Vec<Folder>> {
    let mut stmt = conn.prepare(
        "SELECT f.id, f.name, f.created_at, COUNT(a.id) \
         FROM folders f \
         LEFT JOIN assets a ON a.folder_id = f.id AND a.deleted_at IS NULL \
         GROUP BY f.id ORDER BY f.name COLLATE NOCASE ASC",
    )?;
    let out = stmt
        .query_map([], |r| {
            Ok(Folder {
                id: r.get(0)?,
                name: r.get(1)?,
                created_at: r.get(2)?,
                asset_count: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(out)
}

/// Find a folder name by ID.
pub fn get_name(conn: &Connection, id: i64) -> AppResult<Option<String>> {
    Ok(conn
        .query_row("SELECT name FROM folders WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .optional()?)
}

/// Find a folder by name using the database's case-insensitive uniqueness rules.
pub fn get_by_name(conn: &Connection, name: &str) -> AppResult<Option<Folder>> {
    Ok(conn
        .query_row(
            "SELECT f.id, f.name, f.created_at, COUNT(a.id) \
             FROM folders f LEFT JOIN assets a ON a.folder_id = f.id AND a.deleted_at IS NULL \
             WHERE f.name = ?1 COLLATE NOCASE GROUP BY f.id",
            params![name],
            |r| {
                Ok(Folder {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    created_at: r.get(2)?,
                    asset_count: r.get(3)?,
                })
            },
        )
        .optional()?)
}

/// Create a logical folder.
pub fn insert(conn: &Connection, name: &str, created_at: i64) -> AppResult<i64> {
    conn.execute(
        "INSERT INTO folders(name, created_at) VALUES(?1, ?2)",
        params![name, created_at],
    )
    .map_err(folder_write_error)?;
    Ok(conn.last_insert_rowid())
}

/// Rename a logical folder.
pub fn rename(conn: &Connection, id: i64, new_name: &str) -> AppResult<()> {
    conn.execute(
        "UPDATE folders SET name = ?1 WHERE id = ?2",
        params![new_name, id],
    )
    .map_err(folder_write_error)?;
    Ok(())
}

/// Delete the folder row; foreign keys clear membership after callers handle its assets.
pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute("DELETE FROM folders WHERE id = ?1", params![id])?;
    Ok(())
}

/// Count live member assets for deletion confirmation.
pub fn count_assets(conn: &Connection, id: i64) -> AppResult<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM assets WHERE folder_id = ?1 AND deleted_at IS NULL",
        params![id],
        |r| r.get(0),
    )?)
}
