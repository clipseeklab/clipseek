use crate::commands::with_session;
use crate::db::{self, model::Tag};
use crate::error::{AppError, AppResult};
use crate::vector;
use crate::AppState;

/// List tags sorted by name, with counts.
#[tauri::command]
pub async fn tag_list(app: tauri::AppHandle) -> AppResult<Vec<Tag>> {
    crate::commands::blocking(app, tag_list_blocking).await
}

fn tag_list_blocking(state: &AppState) -> AppResult<Vec<Tag>> {
    with_session(state, |s| {
        let conn = s.read_db();
        db::tags::list(&conn)
    })
}

/// Rename a tag globally and synchronize vector payloads.
#[tauri::command]
pub async fn tag_rename<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
    new_name: String,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| tag_rename_blocking(state, id, new_name)).await
}

pub(crate) fn tag_rename_blocking(state: &AppState, id: i64, new_name: String) -> AppResult<()> {
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() || new_name.contains(',') {
        return Err(AppError::coded(
            "tag_name_invalid",
            "tag name must not be empty and must not contain commas",
        ));
    }
    with_session(state, |s| {
        let affected: Vec<i64> = {
            let conn = s.db.lock().unwrap();
            let mut stmt = conn.prepare("SELECT asset_id FROM asset_tags WHERE tag_id = ?1")?;
            let rows = stmt
                .query_map([id], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            db::tags::rename(&conn, id, &new_name)?;
            rows
        };
        vector::sync_assets_payload(s, &affected)
    })
}

/// Delete a tag and its associations.
#[tauri::command]
pub async fn tag_delete<R: tauri::Runtime>(app: tauri::AppHandle<R>, id: i64) -> AppResult<()> {
    crate::commands::blocking(app, move |state| tag_delete_blocking(state, id)).await
}

pub(crate) fn tag_delete_blocking(state: &AppState, id: i64) -> AppResult<()> {
    with_session(state, |s| {
        let affected: Vec<i64> = {
            let conn = s.db.lock().unwrap();
            let mut stmt = conn.prepare("SELECT asset_id FROM asset_tags WHERE tag_id = ?1")?;
            let ids = stmt
                .query_map([id], |r| r.get(0))?
                .collect::<Result<Vec<_>, _>>()?;
            db::tags::delete(&conn, id)?;
            ids
        };
        vector::sync_assets_payload(s, &affected)
    })
}

/// Create unassigned tags atomically, skipping names already present.
#[tauri::command]
pub async fn tag_create_many<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    names: Vec<String>,
) -> AppResult<usize> {
    if names.is_empty()
        || names.len() > 1000
        || names.iter().any(|n| n.trim().is_empty() || n.contains(','))
    {
        return Err(AppError::coded(
            "tag_name_invalid",
            "Provide 1 to 1000 valid tag names",
        ));
    }
    crate::commands::blocking(app, move |state| with_session(state, |s| {
        let mut conn = s.db.lock().unwrap();
        let tx = conn.transaction()?;
        let mut created = 0;
        for name in names {
            created += tx.execute("INSERT INTO tags(name, created_at) VALUES(?1, ?2) ON CONFLICT(name) DO NOTHING", rusqlite::params![name.trim(),crate::util::now()])?;
        }
        tx.commit()?;
        Ok(created)
    })).await
}

/// Move associations or delete tags in one transaction; never delete assets.
#[tauri::command]
pub async fn tag_batch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
    target_id: Option<i64>,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| with_session(state, |s| {
        let affected = {
            let mut conn = s.db.lock().unwrap();
            let tx = conn.transaction()?;
            if ids.is_empty() || ids.len()>1000 || target_id.is_some_and(|id|ids.contains(&id)) {
                return Err(AppError::coded("tag_name_invalid", "Invalid tag selection"));
            }
            if let Some(target) = target_id {
                let exists:bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM tags WHERE id=?1)",[target],|r|r.get(0))?;
                if !exists { return Err(AppError::coded("tag_name_invalid", "Target tag no longer exists")); }
            }
            let mut affected = std::collections::BTreeSet::new();
            for id in ids {
                let assets = { let mut query=tx.prepare("SELECT asset_id FROM asset_tags WHERE tag_id=?1")?; let rows=query.query_map([id],|r|r.get::<_,i64>(0))?.collect::<Result<Vec<_>,_>>()?; rows };
                affected.extend(assets);
                if let Some(target)=target_id {
                    tx.execute("INSERT INTO asset_tags(asset_id,tag_id) SELECT asset_id,?1 FROM asset_tags WHERE tag_id=?2 ON CONFLICT DO NOTHING",rusqlite::params![target,id])?;
                    tx.execute("DELETE FROM asset_tags WHERE tag_id=?1",[id])?;
                } else { tx.execute("DELETE FROM tags WHERE id=?1",[id])?; }
            }
            tx.commit()?;
            affected.into_iter().collect::<Vec<_>>()
        };
        vector::sync_assets_payload(s, &affected)
    })).await
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetOption {
    id: i64,
    name: String,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetOptions {
    items: Vec<TargetOption>,
    has_more: bool,
}
#[tauri::command]
pub async fn target_options<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    kind: String,
    query: String,
    after: i64,
    exclude: Vec<i64>,
) -> AppResult<TargetOptions> {
    let table = match kind.as_str() {
        "tags" => "tags",
        "folders" => "folders",
        _ => return Err(AppError::InvalidArgument("Invalid target kind".into())),
    };
    if exclude.len() > 1000 {
        return Err(AppError::InvalidArgument(
            "Too many excluded targets".into(),
        ));
    }
    crate::commands::blocking(app,move|state|with_session(state,|s|{
        let conn=s.read_db();
        let mut stmt=conn.prepare(&format!("SELECT id,name FROM {table} WHERE id>?1 AND instr(lower(name),lower(?2))>0 AND id NOT IN (SELECT value FROM json_each(?3)) ORDER BY id LIMIT 51"))?;
        let mut items=stmt.query_map(rusqlite::params![after,query.trim(),serde_json::to_string(&exclude)?],|r|Ok(TargetOption{id:r.get(0)?,name:r.get(1)?}))?.collect::<Result<Vec<_>,_>>()?;
        let has_more=items.len()>50;items.truncate(50);Ok(TargetOptions{items,has_more})
    })).await
}
