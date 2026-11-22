//! Read the current queue and retained attempts; execution stays in the workers.
use crate::error::{AppError, AppResult};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

pub fn schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(include_str!("processing.sql"))?;
    Ok(())
}

#[path = "processing_write.rs"]
mod write;
pub(crate) use write::{atomic, import_stage, refresh_groups, sync_index};
pub use write::{
    finish_batch, record_attempt, recover, register, registered, resume_or_retry_import,
    retry_import, NewAttempt,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: i64,
    pub kind: String,
    pub status: String,
    pub phase: String,
    pub error: Option<String>,
    pub attempt: i64,
    pub index_task_id: Option<i64>,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub updated_at: i64,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub key: String,
    pub batch_id: String,
    pub asset_id: Option<i64>,
    pub file_name: String,
    pub source_path: String,
    pub status: String,
    pub created_at: i64,
    pub tasks: Vec<Task>,
    pub warning_count: i64,
}
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Filter {
    pub status: String,
    pub kind: String,
    pub page: u32,
    pub page_size: u32,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Page {
    pub items: Vec<Item>,
    pub active_count: i64,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub counts: std::collections::HashMap<String, i64>,
    pub revision: i64,
}

/// All list statistics are computed from one row per material, in one snapshot.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub counts: std::collections::HashMap<String, i64>,
    pub active_count: i64,
    pub revision: i64,
}

pub fn summary(conn: &Connection) -> AppResult<Summary> {
    let tx = conn.unchecked_transaction()?;
    let result = read_summary(&tx)?;
    tx.commit()?;
    Ok(result)
}
fn read_summary(conn: &Connection) -> AppResult<Summary> {
    let mut counts = std::collections::HashMap::new();
    let mut active_count = 0;
    let mut stmt =
        conn.prepare("SELECT status,COUNT(*),SUM(active) FROM processing_items GROUP BY status")?;
    for row in stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, i64>(1)?,
            r.get::<_, i64>(2)?,
        ))
    })? {
        let (state, count, active) = row?;
        counts.insert(state, count);
        active_count += active;
    }
    let revision = conn.query_row(
        "SELECT revision FROM processing_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    Ok(Summary {
        counts,
        active_count,
        revision,
    })
}

const TASK_COLS: &str = "id,kind,status,phase,error,attempt,index_task_id,created_at,updated_at";
fn row_task(r: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: r.get(0)?,
        kind: r.get(1)?,
        status: r.get(2)?,
        phase: r.get(3)?,
        error: r.get(4)?,
        attempt: r.get(5)?,
        index_task_id: r.get(6)?,
        created_at: r.get(7)?,
        updated_at: r.get(8)?,
    })
}

pub fn list(conn: &Connection, filter: &Filter) -> AppResult<Page> {
    let page = filter.page.max(1);
    let size = if filter.page_size == 0 {
        20
    } else {
        filter.page_size.min(100)
    };
    let status = filter.status.as_str();
    if !matches!(
        status,
        "" | "all" | "processing" | "attention" | "success" | "cancelled"
    ) {
        return Err(AppError::InvalidArgument(
            "Invalid processing filter".into(),
        ));
    }
    let mask = match filter.kind.as_str() {
        "" | "all" => 0,
        "import" => 1,
        "visual" => 2,
        "subtitles" => 4,
        "warning" => 8,
        _ => {
            return Err(AppError::InvalidArgument(
                "Invalid processing filter".into(),
            ))
        }
    };
    // A single read transaction keeps counts, page and current steps consistent.
    let tx = conn.unchecked_transaction()?;
    let snapshot = read_summary(&tx)?;
    let mut predicates = Vec::new();
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    if !matches!(status, "" | "all") {
        predicates.push("i.status=?");
        args.push(status.to_owned().into());
    }
    if mask != 0 {
        predicates.push("(i.kind_mask & ?) != 0");
        args.push(mask.into());
    }
    let wh = if predicates.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", predicates.join(" AND "))
    };
    let total = tx.query_row(
        &format!("SELECT COUNT(*) FROM processing_items i {wh}"),
        rusqlite::params_from_iter(args.iter()),
        |r| r.get(0),
    )?;
    args.push(i64::from(size).into());
    args.push((i64::from(page - 1) * i64::from(size)).into());
    let mut items = Vec::new();
    let mut owners = std::collections::HashMap::new();
    {
        let mut stmt = tx.prepare(&format!("SELECT i.group_key,i.batch_id,i.asset_id,COALESCE(a.file_name,i.file_name),i.source_path,i.status,i.created_at,i.import_id,i.visual_id,i.subtitles_id,i.warning_id,i.warning_count FROM processing_items i LEFT JOIN assets a ON a.id=i.asset_id {wh} ORDER BY i.updated_at DESC,i.sort_id DESC LIMIT ? OFFSET ?"))?;
        let rows = stmt.query_map(rusqlite::params_from_iter(args.iter()), |r| {
            Ok((
                Item {
                    key: r.get(0)?,
                    batch_id: r.get(1)?,
                    asset_id: r.get(2)?,
                    file_name: r.get(3)?,
                    source_path: r.get(4)?,
                    status: r.get(5)?,
                    created_at: r.get(6)?,
                    tasks: Vec::new(),
                    warning_count: r.get(11)?,
                },
                [
                    r.get::<_, Option<i64>>(7)?,
                    r.get(8)?,
                    r.get(9)?,
                    r.get(10)?,
                ],
            ))
        })?;
        for row in rows {
            let (item, ids) = row?;
            for id in ids.into_iter().flatten() {
                owners.insert(id, items.len());
            }
            items.push(item);
        }
    }
    if !owners.is_empty() {
        let marks = vec!["?"; owners.len()].join(",");
        let mut stmt = tx.prepare(&format!(
            "SELECT {TASK_COLS} FROM processing_tasks WHERE id IN ({marks}) ORDER BY id DESC"
        ))?;
        for task in stmt.query_map(rusqlite::params_from_iter(owners.keys()), row_task)? {
            let task = task?;
            items[owners[&task.id]].tasks.push(task);
        }
    }
    tx.commit()?;
    Ok(Page {
        items,
        active_count: snapshot.active_count,
        total,
        page,
        page_size: size,
        counts: snapshot.counts,
        revision: snapshot.revision,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryPage {
    pub tasks: Vec<Task>,
    pub next_cursor: Option<i64>,
}

pub fn history(conn: &Connection, key: &str, before: Option<i64>) -> AppResult<HistoryPage> {
    let mut stmt = conn.prepare(&format!("SELECT {TASK_COLS} FROM processing_tasks WHERE group_key=?1 AND hidden=0 AND id<?2 UNION ALL SELECT {TASK_COLS} FROM processing_history WHERE group_key=?1 AND hidden=0 AND id<?2 ORDER BY id DESC LIMIT 101"))?;
    let mut tasks = stmt
        .query_map(params![key, before.unwrap_or(i64::MAX)], row_task)?
        .collect::<Result<Vec<_>, _>>()?;
    let more = tasks.len() > 100;
    tasks.truncate(100);
    let next_cursor = if more {
        tasks.last().map(|t| t.id)
    } else {
        None
    };
    Ok(HistoryPage { tasks, next_cursor })
}

pub fn hide(conn: &Connection, key: &str) -> AppResult<()> {
    atomic(conn, || {
        let active: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM processing_items WHERE group_key=? AND active=1)",
            [key],
            |r| r.get(0),
        )?;
        if active {
            return Err(AppError::OperationBusy);
        }
        for table in ["processing_tasks", "processing_history"] {
            conn.execute(
                &format!("UPDATE {table} SET hidden=1 WHERE group_key=? AND hidden=0"),
                [key],
            )?;
        }
        refresh_groups(conn, [key.to_owned()])
    })
}

/// Freeze the ended-material selection until both current and history are hidden.
/// No row triggers can change that selection partway through the operation.
pub fn clear_finished(conn: &Connection) -> AppResult<i64> {
    atomic(conn, || {
        let count = conn.query_row(
            "SELECT COUNT(*) FROM processing_items WHERE active=0",
            [],
            |r| r.get(0),
        )?;
        for table in ["processing_tasks", "processing_history"] {
            conn.execute(&format!("UPDATE {table} SET hidden=1 WHERE hidden=0 AND group_key IN (SELECT group_key FROM processing_items WHERE active=0)"),[])?;
        }
        conn.execute("DELETE FROM processing_items WHERE active=0", [])?;
        if count > 0 {
            conn.execute(
                "UPDATE processing_state SET revision=revision+1 WHERE id=1",
                [],
            )?;
        }
        Ok(count)
    })
}

pub const RETENTION_SECONDS: i64 = 3 * 24 * 60 * 60;
/// History expires independently. Removing it never selects another current step.
pub fn cleanup(conn: &Connection, now: i64, limit: u32) -> AppResult<usize> {
    atomic(conn, || {
        let limit = limit.min(1000);
        let mut q=conn.prepare("DELETE FROM processing_tasks WHERE id IN (SELECT id FROM processing_tasks WHERE updated_at<?1 AND status NOT IN ('queued','running','attention') ORDER BY updated_at,id LIMIT ?2) RETURNING group_key")?;
        let keys = q
            .query_map(params![now - RETENTION_SECONDS, limit], |r| {
                r.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let current = keys.len();
        refresh_groups(conn, keys)?;
        let history=conn.execute("DELETE FROM processing_history WHERE id IN (SELECT id FROM processing_history WHERE updated_at<?1 ORDER BY updated_at,id LIMIT ?2)",params![now-RETENTION_SECONDS,limit-current as u32])?;
        let count = current + history;
        if history > 0 && current == 0 {
            conn.execute(
                "UPDATE processing_state SET revision=revision+1 WHERE id=1",
                [],
            )?;
        }
        conn.execute(
            "UPDATE processing_state SET last_cleanup_at=? WHERE id=1",
            [now],
        )?;
        Ok(count)
    })
}

/// Called by the lightweight polling endpoint; inactive libraries are cleaned on next open.
pub fn maintain(conn: &Connection, now: i64) -> AppResult<()> {
    let last: i64 = conn.query_row(
        "SELECT last_cleanup_at FROM processing_state WHERE id=1",
        [],
        |r| r.get(0),
    )?;
    if now - last >= 60 {
        cleanup(conn, now, 500)?;
    }
    Ok(())
}

/// Fields needed to cancel or retry one current processing step.
#[derive(Debug)]
pub struct ActionTarget {
    pub asset_id: Option<i64>,
    pub source_path: String,
    pub kind: String,
    pub status: String,
    pub index_task_id: Option<i64>,
    pub batch_id: String,
    pub target_folder_id: Option<i64>,
}

pub fn get(conn: &Connection, id: i64) -> AppResult<Option<ActionTarget>> {
    Ok(conn.query_row(
        "SELECT asset_id,source_path,kind,status,index_task_id,batch_id,target_folder_id FROM processing_tasks WHERE id=?",
        [id],
        |row| Ok(ActionTarget {
            asset_id: row.get(0)?, source_path: row.get(1)?, kind: row.get(2)?, status: row.get(3)?,
            index_task_id: row.get(4)?, batch_id: row.get(5)?, target_folder_id: row.get(6)?,
        }),
    ).optional()?)
}
