//! Transactional writes. Current steps never depend on retained history.
use super::*;
use std::collections::HashSet;

/// SAVEPOINTs compose with the caller's transaction and roll back on errors/panics.
pub(crate) fn atomic<T>(conn: &Connection, f: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
    struct Guard<'a>(&'a Connection, bool);
    impl Drop for Guard<'_> {
        fn drop(&mut self) {
            if !self.1 {
                let _ = self
                    .0
                    .execute_batch("ROLLBACK TO processing_write; RELEASE processing_write;");
            }
        }
    }
    conn.execute_batch("SAVEPOINT processing_write;")?;
    let mut guard = Guard(conn, false);
    let value = f()?;
    conn.execute_batch("RELEASE processing_write;")?;
    guard.1 = true;
    Ok(value)
}

#[derive(Debug, Default, Clone)]
pub struct NewAttempt {
    pub item_key: String,
    pub batch_id: String,
    pub asset_id: Option<i64>,
    pub file_name: String,
    pub source_path: String,
    pub kind: String,
    pub status: String,
    pub phase: String,
    pub error: Option<String>,
    pub attempt: i64,
    pub index_task_id: Option<i64>,
    pub notice_id: Option<i64>,
    pub target_folder_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

const RECORD_COLS: &str = "id,item_key,batch_id,asset_id,file_name,source_path,kind,status,phase,error,attempt,index_task_id,notice_id,target_folder_id,created_at,updated_at,hidden,group_key";

struct Record {
    id: i64,
    data: NewAttempt,
    key: String,
}
fn row_record(r: &rusqlite::Row<'_>) -> rusqlite::Result<Record> {
    Ok(Record {
        id: r.get(0)?,
        data: NewAttempt {
            item_key: r.get(1)?,
            batch_id: r.get(2)?,
            asset_id: r.get(3)?,
            file_name: r.get(4)?,
            source_path: r.get(5)?,
            kind: r.get(6)?,
            status: r.get(7)?,
            phase: r.get(8)?,
            error: r.get(9)?,
            attempt: r.get(10)?,
            index_task_id: r.get(11)?,
            notice_id: r.get(12)?,
            target_folder_id: r.get(13)?,
            created_at: r.get(14)?,
            updated_at: r.get(15)?,
        },
        key: r.get(17)?,
    })
}
fn records(
    conn: &Connection,
    predicate: &str,
    args: impl rusqlite::Params,
) -> AppResult<Vec<Record>> {
    let mut q = conn.prepare(&format!(
        "SELECT {RECORD_COLS} FROM processing_tasks WHERE {predicate}"
    ))?;
    let rows = q
        .query_map(args, row_record)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
fn key(data: &NewAttempt) -> String {
    data.asset_id
        .map(|id| format!("asset:{id}"))
        .unwrap_or_else(|| format!("item:{}", data.item_key))
}
fn archive(conn: &Connection, id: i64) -> AppResult<()> {
    conn.execute(&format!("INSERT INTO processing_history({RECORD_COLS}) SELECT {RECORD_COLS} FROM processing_tasks WHERE id=?"), [id])?;
    conn.execute("DELETE FROM processing_tasks WHERE id=?", [id])?;
    Ok(())
}

/// Start an attempt and archive its predecessor. The caller owns the savepoint.
fn insert(conn: &Connection, data: &NewAttempt) -> AppResult<i64> {
    insert_at(conn, data, &key(data))
}
fn insert_at(conn: &Connection, data: &NewAttempt, group: &str) -> AppResult<i64> {
    if data.kind != "warning" {
        for old in records(conn, "group_key=?1 AND kind=?2", params![group, data.kind])? {
            archive(conn, old.id)?;
        }
    }
    conn.execute("INSERT INTO processing_tasks(item_key,batch_id,asset_id,file_name,source_path,kind,status,phase,error,attempt,index_task_id,notice_id,target_folder_id,created_at,updated_at,group_key)
        VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![data.item_key,data.batch_id,data.asset_id,data.file_name,data.source_path,data.kind,data.status,data.phase,data.error,data.attempt.max(1),data.index_task_id,data.notice_id,data.target_folder_id,data.created_at,data.updated_at,group])?;
    Ok(conn.last_insert_rowid())
}

/// Shared entry for a new import/index/warning attempt.
pub fn record_attempt(conn: &Connection, data: NewAttempt) -> AppResult<i64> {
    atomic(conn, || {
        let id = insert(conn, &data)?;
        refresh_groups(conn, [key(&data)])?;
        Ok(id)
    })
}

/// Refresh each affected material once, using only its current steps.
pub(crate) fn refresh_groups(
    conn: &Connection,
    keys: impl IntoIterator<Item = String>,
) -> AppResult<()> {
    let keys: HashSet<_> = keys.into_iter().collect();
    if keys.is_empty() {
        return Ok(());
    }
    for group in &keys {
        let current = records(conn, "group_key=? AND hidden=0 ORDER BY id", [group])?;
        let Some(last) = current.last() else {
            conn.execute("DELETE FROM processing_items WHERE group_key=?", [group])?;
            continue;
        };
        let active = current
            .iter()
            .any(|r| matches!(r.data.status.as_str(), "queued" | "running"));
        let attention = current.iter().any(|r| {
            matches!(
                r.data.status.as_str(),
                "failed" | "attention" | "interrupted" | "no_source"
            )
        });
        let status = if attention {
            "attention"
        } else if active {
            "processing"
        } else if current.iter().any(|r| r.data.status == "cancelled") {
            "cancelled"
        } else {
            "success"
        };
        let step = |kind: &str| {
            current
                .iter()
                .rev()
                .find(|r| r.data.kind == kind)
                .map(|r| r.id)
        };
        let warning = current
            .iter()
            .rev()
            .find(|r| r.data.kind == "warning" && r.data.status == "attention")
            .map(|r| r.id)
            .or_else(|| step("warning"));
        let warning_count = current
            .iter()
            .filter(|r| r.data.kind == "warning" && r.data.status == "attention")
            .count();
        let mask = [
            ("import", 1),
            ("visual", 2),
            ("subtitles", 4),
            ("warning", 8),
        ]
        .into_iter()
        .filter(|(kind, _)| step(kind).is_some())
        .map(|(_, bit)| bit)
        .sum::<i64>();
        let created = current.iter().map(|r| r.data.created_at).min().unwrap();
        let updated = current.iter().map(|r| r.data.updated_at).max().unwrap();
        let source = current
            .iter()
            .rev()
            .find(|r| !r.data.source_path.is_empty())
            .unwrap_or(last);
        conn.execute("INSERT INTO processing_items(group_key,batch_id,asset_id,file_name,source_path,created_at,updated_at,sort_id,status,active,kind_mask,import_id,visual_id,subtitles_id,warning_id,warning_count)
            VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)
            ON CONFLICT(group_key) DO UPDATE SET batch_id=excluded.batch_id,asset_id=excluded.asset_id,file_name=excluded.file_name,source_path=excluded.source_path,created_at=excluded.created_at,updated_at=excluded.updated_at,sort_id=excluded.sort_id,status=excluded.status,active=excluded.active,kind_mask=excluded.kind_mask,import_id=excluded.import_id,visual_id=excluded.visual_id,subtitles_id=excluded.subtitles_id,warning_id=excluded.warning_id,warning_count=excluded.warning_count",
            params![group,last.data.batch_id,last.data.asset_id,last.data.file_name,source.data.source_path,created,updated,last.id,status,active,mask,step("import"),step("visual"),step("subtitles"),warning,warning_count as i64])?;
    }
    conn.execute(
        "UPDATE processing_state SET revision=revision+1 WHERE id=1",
        [],
    )?;
    Ok(())
}

pub fn register(
    conn: &Connection,
    batch: &str,
    path: &std::path::Path,
    folder_id: Option<i64>,
) -> AppResult<i64> {
    atomic(conn, || {
        let item_key = format!("import:{batch}:{}", path.to_string_lossy());
        let now = crate::util::now();
        let previous = records(conn, "item_key=? AND kind='import'", [&item_key])?.pop();
        // Material deletion clears asset_id but must not change this registered item's identity.
        let group = previous
            .as_ref()
            .map(|r| r.key.clone())
            .unwrap_or_else(|| format!("item:{item_key}"));
        let data = if let Some(previous) = previous {
            if !matches!(
                previous.data.status.as_str(),
                "interrupted" | "failed" | "cancelled"
            ) {
                return Ok(previous.id);
            }
            NewAttempt {
                status: "queued".into(),
                phase: "copy".into(),
                error: None,
                attempt: previous.data.attempt + 1,
                created_at: now,
                updated_at: now,
                ..previous.data
            }
        } else {
            NewAttempt {
                item_key,
                batch_id: batch.into(),
                file_name: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into(),
                source_path: path.to_string_lossy().into(),
                kind: "import".into(),
                status: "queued".into(),
                phase: "copy".into(),
                target_folder_id: folder_id,
                attempt: 1,
                created_at: now,
                updated_at: now,
                ..Default::default()
            }
        };
        let id = insert_at(conn, &data, &group)?;
        refresh_groups(conn, [group])?;
        Ok(id)
    })
}

/// Associate pre-asset work once. Newer current steps win; old attempts stay in history.
fn bind_asset(conn: &Connection, old_key: &str, asset: i64) -> AppResult<()> {
    let group = format!("asset:{asset}");
    if old_key == group {
        return Ok(());
    }
    conn.execute(
        "UPDATE processing_history SET asset_id=?1,group_key=?2 WHERE group_key=?3",
        params![asset, group, old_key],
    )?;
    for row in records(conn, "group_key=?", [old_key])? {
        if row.data.kind != "warning" {
            if let Some(other) = records(
                conn,
                "group_key=?1 AND kind=?2",
                params![group, row.data.kind],
            )?
            .pop()
            {
                if other.id > row.id {
                    archive(conn, row.id)?;
                    continue;
                }
                archive(conn, other.id)?;
            }
        }
        conn.execute(
            "UPDATE processing_tasks SET asset_id=?1,group_key=?2 WHERE id=?3",
            params![asset, group, row.id],
        )?;
    }
    // Rows archived by the merge also retain the resolved identity.
    conn.execute(
        "UPDATE processing_history SET asset_id=?1,group_key=?2 WHERE group_key=?3",
        params![asset, group, old_key],
    )?;
    Ok(())
}

pub fn registered(
    conn: &Connection,
    id: i64,
    asset: Option<i64>,
    status: &str,
    error: Option<&str>,
) -> AppResult<()> {
    atomic(conn, || {
        let current = records(conn, "id=?", [id])?.pop();
        let mut keys = Vec::new();
        if let Some(current) = current {
            keys.push(current.key.clone());
            if let Some(asset) = asset {
                bind_asset(conn, &current.key, asset)?;
                keys.push(format!("asset:{asset}"));
            }
        }
        // Late completion of a superseded attempt updates audit history only.
        for table in ["processing_tasks", "processing_history"] {
            conn.execute(
                &format!("UPDATE {table} SET status=?2,error=?3,updated_at=?4 WHERE id=?1"),
                params![id, status, error, crate::util::now()],
            )?;
        }
        refresh_groups(conn, keys)
    })
}

pub fn finish_batch(
    conn: &Connection,
    batch: &str,
    status: &str,
    error: Option<&str>,
) -> AppResult<()> {
    atomic(conn, || {
        let rows = records(conn, "batch_id=?1 AND kind='import' AND (status IN ('queued','running') OR (?2='cancelled' AND status='interrupted'))", params![batch,status])?;
        conn.execute("UPDATE processing_tasks SET status=?2,error=COALESCE(?3,error),updated_at=?4 WHERE batch_id=?1 AND kind='import' AND (status IN ('queued','running') OR (?2='cancelled' AND status='interrupted'))", params![batch,status,error,crate::util::now()])?;
        refresh_groups(conn, rows.into_iter().map(|r| r.key))
    })
}

pub(crate) fn import_stage(conn: &Connection, asset: i64, stage: &str) -> AppResult<()> {
    let rows = records(
        conn,
        "asset_id=? AND kind='import' AND status IN ('queued','running')",
        [asset],
    )?;
    conn.execute("UPDATE processing_tasks SET phase=CASE WHEN ?2 LIKE 'error%' THEN phase ELSE ?2 END,
        status=CASE WHEN ?2 LIKE 'error%' THEN 'failed' WHEN ?2='done' THEN 'completed' ELSE 'running' END,
        error=CASE WHEN ?2 LIKE 'error%' THEN substr(?2,7) ELSE NULL END,updated_at=?3
        WHERE asset_id=?1 AND kind='import' AND status IN ('queued','running')", params![asset,stage,crate::util::now()])?;
    refresh_groups(conn, rows.into_iter().map(|r| r.key))
}

pub fn retry_import(conn: &Connection, asset: i64) -> AppResult<()> {
    atomic(conn, || {
        let previous = records(conn, "asset_id=? AND kind='import'", [asset])?.pop();
        if previous
            .as_ref()
            .is_some_and(|r| matches!(r.data.status.as_str(), "queued" | "running"))
        {
            return Err(AppError::OperationBusy);
        }
        let (file_name,source_path): (String,String) = conn.query_row("SELECT file_name,COALESCE(source_path,rel_path) FROM assets WHERE id=? AND deleted_at IS NULL", [asset], |r|Ok((r.get(0)?,r.get(1)?)))?;
        let now = crate::util::now();
        let source_path = previous
            .as_ref()
            .map(|r| r.data.source_path.clone())
            .unwrap_or(source_path);
        let mut data = previous.map(|r| r.data).unwrap_or_else(|| NewAttempt {
            item_key: format!("retry-import-{asset}"),
            batch_id: "retry".into(),
            ..Default::default()
        });
        data.asset_id = Some(asset);
        data.file_name = file_name;
        data.source_path = source_path;
        data.kind = "import".into();
        data.status = "queued".into();
        data.phase = "pending".into();
        data.error = None;
        data.attempt += 1;
        data.created_at = now;
        data.updated_at = now;
        record_attempt(conn, data)?;
        Ok(())
    })
}

pub fn resume_or_retry_import(conn: &Connection, asset: i64) -> AppResult<()> {
    if records(
        conn,
        "asset_id=? AND kind='import' AND status IN ('queued','running')",
        [asset],
    )?
    .is_empty()
    {
        retry_import(conn, asset)?;
    }
    Ok(())
}

struct IndexTaskSnapshot {
    asset_id: i64,
    kind: String,
    status: String,
    error: Option<String>,
    origin: String,
    batch_id: Option<String>,
    created_at: i64,
    updated_at: i64,
    file_name: String,
    source_path: String,
}

/// Called only by the indexing DAO, inside its execution-state transaction.
pub(crate) fn sync_index(
    conn: &Connection,
    id: i64,
    new_attempt: bool,
) -> AppResult<Option<String>> {
    let IndexTaskSnapshot {
        asset_id: asset, kind, status, error, origin, batch_id: batch,
        created_at: created, updated_at: updated, file_name: file, source_path: path,
    } = conn.query_row(
        "SELECT t.asset_id,t.kind,t.status,t.error,t.origin,t.batch_id,t.created_at,t.updated_at,a.file_name,COALESCE(a.source_path,a.rel_path) FROM index_tasks t JOIN assets a ON a.id=t.asset_id WHERE t.id=?",
        [id],
        |row| Ok(IndexTaskSnapshot {
            asset_id: row.get(0)?, kind: row.get(1)?, status: row.get(2)?, error: row.get(3)?,
            origin: row.get(4)?, batch_id: row.get(5)?, created_at: row.get(6)?, updated_at: row.get(7)?,
            file_name: row.get(8)?, source_path: row.get(9)?,
        }),
    )?;
    let group = format!("asset:{asset}");
    let current = records(conn, "group_key=?1 AND kind=?2", params![group, kind])?.pop();
    if !new_attempt
        && current
            .as_ref()
            .is_some_and(|r| r.data.index_task_id == Some(id))
    {
        conn.execute(
            "UPDATE processing_tasks SET status=?2,error=?3,updated_at=?4,hidden=0 WHERE id=?1",
            params![current.unwrap().id, status, error, updated],
        )?;
    } else if new_attempt || (current.is_none() && matches!(status.as_str(), "queued" | "running"))
    {
        let previous = records(conn, "asset_id=? AND kind='import'", [asset])?.pop();
        let attempt = current
            .as_ref()
            .filter(|r| r.data.index_task_id == Some(id))
            .map_or(1, |r| r.data.attempt + 1);
        let item_key = if origin == "manual" {
            format!("build:{}:{asset}", batch.as_deref().unwrap_or("index"))
        } else {
            previous
                .as_ref()
                .map(|r| r.data.item_key.clone())
                .unwrap_or_else(|| format!("index:{id}"))
        };
        let batch_id = if origin == "manual" {
            batch.unwrap_or_else(|| "index".into())
        } else {
            previous
                .as_ref()
                .map(|r| r.data.batch_id.clone())
                .unwrap_or_else(|| "automatic-index".into())
        };
        let phase = kind.clone();
        insert(
            conn,
            &NewAttempt {
                item_key,
                batch_id,
                asset_id: Some(asset),
                file_name: file,
                source_path: previous
                    .as_ref()
                    .map(|r| r.data.source_path.clone())
                    .unwrap_or(path),
                kind,
                status,
                phase,
                error,
                attempt,
                index_task_id: Some(id),
                created_at: if new_attempt {
                    crate::util::now()
                } else {
                    created
                },
                updated_at: updated,
                ..Default::default()
            },
        )?;
    } else {
        return Ok(None);
    }
    Ok(Some(group))
}

pub fn recover(conn: &Connection) -> AppResult<()> {
    atomic(conn, || {
        let rows = records(conn, "kind='import' AND status IN ('queued','running')", [])?;
        conn.execute("UPDATE processing_tasks SET status='interrupted',error='Processing interrupted; resume the import or retry this file',updated_at=? WHERE kind='import' AND status IN ('queued','running')",[crate::util::now()])?;
        refresh_groups(conn, rows.into_iter().map(|r| r.key))
    })
}
