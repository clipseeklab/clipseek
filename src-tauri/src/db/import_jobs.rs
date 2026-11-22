use rusqlite::{params, Connection};
use serde::Serialize;

use crate::error::AppResult;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportJob {
    pub id: String,
    pub mode: String,
    pub paths: Vec<String>,
    pub folder_id: Option<i64>,
    pub status: String,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub duplicates: usize,
    pub created_at: i64,
    pub index_policy: super::indexing::IndexPolicy,
    pub tags: Vec<String>,
}

/// Durable handoff evidence survives the removal of completed import_jobs rows.
pub fn has_submission(conn: &Connection, id: &str) -> AppResult<bool> {
    Ok(conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id=?1) OR EXISTS(SELECT 1 FROM processing_tasks WHERE batch_id=?1) OR EXISTS(SELECT 1 FROM processing_history WHERE batch_id=?1)",
        [id], |row| row.get(0),
    )?)
}

pub fn start(
    conn: &Connection,
    id: &str,
    mode: &str,
    paths: &[String],
    folder_id: Option<i64>,
) -> AppResult<()> {
    let now = crate::util::now();
    let paths_json = serde_json::to_string(paths)?;
    conn.execute(
        "INSERT INTO import_jobs(id,mode,paths_json,folder_id,status,created_at,updated_at)
         VALUES(?1,?2,?3,?4,'running',?5,?5)
         ON CONFLICT(id) DO UPDATE SET
           mode=excluded.mode,
           paths_json=excluded.paths_json,
           folder_id=excluded.folder_id,
           status='running',
           updated_at=?5",
        params![id, mode, paths_json, folder_id, now],
    )?;
    Ok(())
}

pub fn finish(
    conn: &Connection,
    id: &str,
    status: &str,
    total: usize,
    done: usize,
    failed: usize,
    duplicates: usize,
) -> AppResult<()> {
    if matches!(status, "completed" | "failed") {
        return super::processing::atomic(conn, || {
            conn.execute("DELETE FROM import_jobs WHERE id=?1", params![id])?;
            clear_tags(conn, id)
        });
    }
    conn.execute(
        "UPDATE import_jobs SET status=?2,total=?3,done=?4,failed=?5,duplicates=?6,updated_at=?7
         WHERE id=?1",
        params![
            id,
            status,
            total as i64,
            done as i64,
            failed as i64,
            duplicates as i64,
            crate::util::now()
        ],
    )?;
    Ok(())
}

pub fn recoverable(conn: &Connection) -> AppResult<Vec<ImportJob>> {
    let mut stmt = conn.prepare(
        "SELECT id,mode,paths_json,folder_id,status,total,done,failed,duplicates,created_at,visual_index,subtitle_index,
                COALESCE((SELECT value FROM meta WHERE key='import_job_tags.v1.' || import_jobs.id),'[]')
         FROM import_jobs WHERE status IN ('queued','running','interrupted')
         ORDER BY created_at",
    )?;
    let rows = stmt.query_map([], |row| {
        let paths_json: String = row.get(2)?;
        Ok(ImportJob {
            id: row.get(0)?,
            mode: row.get(1)?,
            paths: serde_json::from_str(&paths_json).unwrap_or_default(),
            folder_id: row.get(3)?,
            status: "interrupted".into(),
            total: row.get::<_, i64>(5)?.max(0) as usize,
            done: row.get::<_, i64>(6)?.max(0) as usize,
            failed: row.get::<_, i64>(7)?.max(0) as usize,
            duplicates: row.get::<_, i64>(8)?.max(0) as usize,
            created_at: row.get(9)?,
            tags: serde_json::from_str(&row.get::<_, String>(12)?).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    12,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
            index_policy: super::indexing::IndexPolicy {
                visual: row.get(10)?,
                subtitles: row.get(11)?,
            },
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<ImportJob>> {
    Ok(recoverable(conn)?.into_iter().find(|job| job.id == id))
}

pub fn abandon(conn: &Connection, id: &str) -> AppResult<()> {
    super::processing::atomic(conn, || {
        super::processing::finish_batch(conn, id, "cancelled", None)?;
        conn.execute("DELETE FROM import_jobs WHERE id=?1", params![id])?;
        clear_tags(conn, id)
    })
}

/// Stored with the job transaction, without changing the library schema.
pub fn set_tags(conn: &Connection, id: &str, tags: &[String]) -> AppResult<()> {
    let exists = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM import_jobs WHERE id=?1)",
        [id],
        |r| r.get::<_, bool>(0),
    )?;
    if !exists {
        return Err(crate::error::AppError::InvalidArgument(
            "import job does not exist".into(),
        ));
    }
    let mut clean = Vec::new();
    for name in tags
        .iter()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty() && !s.contains(','))
    {
        if !clean.contains(&name) {
            clean.push(name);
        }
    }
    if clean.is_empty() {
        return clear_tags(conn, id);
    }
    conn.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![format!("import_job_tags.v1.{id}"), serde_json::to_string(&clean)?])?;
    Ok(())
}

fn clear_tags(conn: &Connection, id: &str) -> AppResult<()> {
    conn.execute(
        "DELETE FROM meta WHERE key=?1",
        [format!("import_job_tags.v1.{id}")],
    )?;
    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationStatus {
    pub total: i64,
    pub active: i64,
    pub failed: i64,
    pub completed: i64,
}
pub fn operation_status(conn: &Connection, batch: &str) -> AppResult<Option<OperationStatus>> {
    let result=conn.query_row("WITH attempts AS (
        SELECT item_key,status,updated_at,attempt,id FROM processing_tasks WHERE batch_id=?1
        UNION ALL SELECT item_key,status,updated_at,attempt,id FROM processing_history WHERE batch_id=?1
    ), latest AS (SELECT status,ROW_NUMBER() OVER(PARTITION BY item_key ORDER BY updated_at DESC,attempt DESC,id DESC) AS position FROM attempts)
    SELECT COUNT(*),COALESCE(SUM(status IN ('queued','running')),0),COALESCE(SUM(status IN ('failed','interrupted','cancelled','no_source','attention')),0),COALESCE(SUM(status IN ('completed','duplicate','skipped','ignored')),0) FROM latest WHERE position=1",[batch],|r|Ok(OperationStatus{total:r.get(0)?,active:r.get(1)?,failed:r.get(2)?,completed:r.get(3)?}))?;
    Ok((result.total > 0).then_some(result))
}

#[cfg(test)]
#[path = "../../tests/unit/import_jobs.rs"]
mod tests;
