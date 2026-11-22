use crate::{
    db::model::AssetFilter,
    error::{AppError, AppResult},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IndexPolicy {
    pub visual: bool,
    pub subtitles: bool,
}
impl Default for IndexPolicy {
    fn default() -> Self {
        Self {
            visual: true,
            subtitles: false,
        }
    }
}

pub fn has_column(conn: &Connection, table: &str, column: &str) -> bool {
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
        params![table, column],
        |r| r.get(0),
    )
    .unwrap_or(false)
}
pub fn schema(conn: &Connection) -> AppResult<()> {
    for (column, default) in [
        ("visual_indexed", 0),
        ("subtitle_indexed", 0),
        ("visual_requested", 1),
        ("subtitle_requested", 1),
    ] {
        if !has_column(conn, "assets", column) {
            conn.execute(
                &format!(
                    "ALTER TABLE assets ADD COLUMN {column} INTEGER NOT NULL DEFAULT {default}"
                ),
                [],
            )?;
        }
    }
    for column in ["visual_index", "subtitle_index"] {
        if has_column(conn, "import_jobs", "id") && !has_column(conn, "import_jobs", column) {
            conn.execute(
                &format!("ALTER TABLE import_jobs ADD COLUMN {column} INTEGER NOT NULL DEFAULT 1"),
                [],
            )?;
        }
    }
    conn.execute_batch("CREATE TABLE IF NOT EXISTS index_tasks(
        id INTEGER PRIMARY KEY AUTOINCREMENT, asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
        kind TEXT NOT NULL CHECK(kind IN ('visual','subtitles')), status TEXT NOT NULL DEFAULT 'queued',
        error TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL);
        CREATE INDEX IF NOT EXISTS index_tasks_asset ON index_tasks(asset_id,kind,id DESC);
        CREATE UNIQUE INDEX IF NOT EXISTS index_tasks_active ON index_tasks(asset_id,kind) WHERE status IN ('queued','running');
        CREATE TABLE IF NOT EXISTS index_versions(asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE, kind TEXT NOT NULL, recipe TEXT NOT NULL, source_size INTEGER NOT NULL, source_mtime TEXT NOT NULL, PRIMARY KEY(asset_id,kind));
        CREATE TABLE IF NOT EXISTS subtitle_sources(asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,name TEXT NOT NULL,raw BLOB NOT NULL);
")?;
    if has_column(conn, "assets", "deleted_at") {
        conn.execute_batch("CREATE TRIGGER IF NOT EXISTS cancel_indexes_on_trash AFTER UPDATE OF deleted_at ON assets WHEN NEW.deleted_at IS NOT NULL BEGIN UPDATE index_tasks SET status='cancelled' WHERE asset_id=NEW.id AND status IN ('queued','running'); END;")?;
        conn.execute_batch("CREATE INDEX IF NOT EXISTS assets_visual_indexed ON assets(visual_indexed,deleted_at); CREATE INDEX IF NOT EXISTS assets_subtitle_indexed ON assets(subtitle_indexed,deleted_at);")?;
    }
    if has_column(conn, "frames", "embedded") && has_column(conn, "assets", "import_stage") {
        conn.execute("UPDATE assets SET visual_indexed=1 WHERE import_stage IN ('done','embedding') AND EXISTS(SELECT 1 FROM frames f WHERE f.asset_id=assets.id) AND NOT EXISTS(SELECT 1 FROM frames f WHERE f.asset_id=assets.id AND f.embedded=0)", [])?;
    }
    conn.execute("UPDATE assets SET subtitle_indexed=1 WHERE EXISTS(SELECT 1 FROM subtitle_tracks t JOIN subtitle_docs d ON d.asset_id=t.asset_id AND d.version=t.version WHERE t.asset_id=assets.id)", [])?;
    conn.execute("INSERT OR IGNORE INTO subtitle_sources SELECT asset_id,source_name,raw FROM subtitle_tracks", [])?;
    Ok(())
}

pub fn policy(conn: &Connection, id: i64) -> AppResult<IndexPolicy> {
    Ok(conn.query_row(
        "SELECT visual_requested,subtitle_requested FROM assets WHERE id=?",
        [id],
        |r| {
            Ok(IndexPolicy {
                visual: r.get(0)?,
                subtitles: r.get(1)?,
            })
        },
    )?)
}
pub fn set_policy(conn: &Connection, id: i64, policy: IndexPolicy) -> AppResult<()> {
    conn.execute(
        "UPDATE assets SET visual_requested=?1,subtitle_requested=?2 WHERE id=?3",
        params![policy.visual, policy.subtitles, id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
pub struct IndexScope {
    pub asset_ids: Vec<i64>,
    pub folder_ids: Vec<i64>,
    pub tag_ids: Vec<i64>,
    pub all: bool,
    pub filter: Option<AssetFilter>,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexTask {
    pub id: i64,
    pub asset_id: i64,
    pub file_name: String,
    pub kind: String,
    pub status: String,
    pub error: Option<String>,
}

/// Backend snapshot: scope expansion includes all assets, irrespective of UI pagination.
/// Validate reference generations during explicit source checks and build requests.
pub fn invalidate_changed_references(conn: &Connection) -> AppResult<()> {
    let mut q=conn.prepare("SELECT v.asset_id,v.kind,a.source_path,v.source_size,v.source_mtime FROM index_versions v JOIN assets a ON a.id=v.asset_id WHERE a.source_path IS NOT NULL AND a.deleted_at IS NULL")?;
    let rows = q
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (id, kind, path, size, time) in rows {
        let Ok(meta) = std::fs::metadata(path) else {
            continue;
        };
        let current = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|t| t.as_nanos().to_string());
        if meta.len() as i64 != size || current.as_deref() != Some(&time) {
            let column = if kind == "visual" {
                "visual_indexed"
            } else {
                "subtitle_indexed"
            };
            conn.execute(&format!("UPDATE assets SET {column}=0 WHERE id=?"), [id])?;
        }
    }
    Ok(())
}

pub fn enqueue_scope(conn: &Connection, scope: &IndexScope, kinds: &[String]) -> AppResult<usize> {
    super::processing::atomic(conn, || enqueue_scope_inner(conn, scope, kinds))
}
fn enqueue_scope_inner(
    conn: &Connection,
    scope: &IndexScope,
    kinds: &[String],
) -> AppResult<usize> {
    if kinds.is_empty() || kinds.iter().any(|k| k.as_str() != "visual") {
        return Err(AppError::InvalidArgument(
            "Choose visual and/or subtitles".into(),
        ));
    }
    if !scope.all
        && scope.asset_ids.is_empty()
        && scope.folder_ids.is_empty()
        && scope.tag_ids.is_empty()
        && scope.filter.is_none()
    {
        return Err(AppError::InvalidArgument("Empty indexing scope".into()));
    }
    invalidate_changed_references(conn)?;
    let filter = scope.filter.clone().unwrap_or_default();
    let (mut wh, mut values) = crate::db::assets::build_where(&filter);
    // Tasks may never include trash, including filters supplied by external callers.
    wh.push_str(" AND a.deleted_at IS NULL");
    if !scope.all && scope.filter.is_none() {
        let mut parts = Vec::new();
        for (ids, expression) in [
            (&scope.asset_ids, "a.id"),
            (&scope.folder_ids, "a.folder_id"),
            (
                &scope.tag_ids,
                "a.id IN (SELECT asset_id FROM asset_tags WHERE tag_id",
            ),
        ] {
            if ids.is_empty() {
                continue;
            }
            let marks = vec!["?"; ids.len()].join(",");
            parts.push(if expression.contains("SELECT") {
                format!("{expression} IN ({marks}))")
            } else {
                format!("{expression} IN ({marks})")
            });
            values.extend(ids.iter().map(|id| rusqlite::types::Value::Integer(*id)));
        }
        wh.push_str(&format!(" AND ({})", parts.join(" OR ")));
    }
    let batch = crate::util::new_id();
    let mut count = 0;
    let mut keys = Vec::new();
    for target in kinds {
        let column = if target == "visual" {
            "visual_indexed"
        } else {
            "subtitle_indexed"
        };
        let sql=format!("INSERT OR IGNORE INTO index_tasks(asset_id,kind,status,created_at,updated_at,origin,batch_id) SELECT DISTINCT a.id,?, 'queued',?,?,'manual',? FROM assets a WHERE {wh} AND a.{column}=0 {}",if target=="subtitles" {"AND a.kind='video' AND EXISTS(SELECT 1 FROM subtitle_sources ss WHERE ss.asset_id=a.id)"} else {""});
        let now = crate::util::now();
        let mut args = vec![
            rusqlite::types::Value::Text(target.clone()),
            now.into(),
            now.into(),
            rusqlite::types::Value::Text(batch.clone()),
        ];
        args.extend(values.clone());
        let mut q = conn.prepare(&(sql + " RETURNING id"))?;
        let ids = q
            .query_map(rusqlite::params_from_iter(args), |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        count += ids.len();
        for id in ids {
            keys.extend(super::processing::sync_index(conn, id, true)?);
        }
    }
    super::processing::refresh_groups(conn, keys)?;
    Ok(count)
}
pub fn enqueue(conn: &Connection, id: i64, kind: &str) -> AppResult<usize> {
    let column = match kind {
        "visual" => "visual_indexed",
        "subtitles" => "subtitle_indexed",
        _ => return Err(AppError::InvalidArgument("Invalid index kind".into())),
    };
    super::processing::atomic(conn, || {
        let mut q=conn.prepare(&format!("INSERT OR IGNORE INTO index_tasks(asset_id,kind,status,created_at,updated_at) SELECT id,?1,'queued',?2,?2 FROM assets WHERE id=?3 AND deleted_at IS NULL AND {column}=0 RETURNING id"))?;
        let ids = q
            .query_map(params![kind, crate::util::now(), id], |r| {
                r.get::<_, i64>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut keys = Vec::new();
        for id in &ids {
            keys.extend(super::processing::sync_index(conn, *id, true)?);
        }
        super::processing::refresh_groups(conn, keys)?;
        Ok(ids.len())
    })
}

/// All execution-state transitions update queue snapshots in the same transaction.
/// `allowed` fences late workers and makes repeated cancel/retry calls idempotent.
pub fn set_status(
    conn: &Connection,
    id: Option<i64>,
    status: &str,
    error: Option<&str>,
    allowed: &[&str],
) -> AppResult<usize> {
    set_status_inner(conn, id, status, error, allowed, None)
}

/// Fence delayed workers after cancellation and a retry of the same execution row.
pub fn set_status_if_current(
    conn: &Connection,
    id: i64,
    status: &str,
    error: Option<&str>,
    allowed: &[&str],
    attempt: i64,
) -> AppResult<usize> {
    set_status_inner(conn, Some(id), status, error, allowed, Some(attempt))
}
fn set_status_inner(
    conn: &Connection,
    id: Option<i64>,
    status: &str,
    error: Option<&str>,
    allowed: &[&str],
    attempt: Option<i64>,
) -> AppResult<usize> {
    if allowed.is_empty() {
        return Ok(0);
    }
    super::processing::atomic(conn, || {
        if let Some(attempt) = attempt {
            let current: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM processing_tasks WHERE id=?1 AND index_task_id=?2)",
                params![attempt, id],
                |r| r.get(0),
            )?;
            if !current {
                return Ok(0);
            }
        }
        let marks = vec!["?"; allowed.len()].join(",");
        let mut args: Vec<rusqlite::types::Value> = vec![id.into()];
        args.extend(
            allowed
                .iter()
                .map(|s| rusqlite::types::Value::Text((*s).into())),
        );
        let mut q = conn.prepare(&format!(
            "SELECT id,status FROM index_tasks WHERE (?1 IS NULL OR id=?1) AND status IN ({marks})"
        ))?;
        let targets = q
            .query_map(rusqlite::params_from_iter(args), |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        let mut count = 0;
        let mut keys = Vec::new();
        for (id, old) in targets {
            let changed=conn.execute("UPDATE OR IGNORE index_tasks SET status=?2,error=?3,updated_at=?4 WHERE id=?1 AND status=?5 AND (?2<>'queued' OR EXISTS(SELECT 1 FROM assets a WHERE a.id=index_tasks.asset_id AND a.deleted_at IS NULL))",params![id,status,error,crate::util::now(),old])?;
            if changed > 0 {
                keys.extend(super::processing::sync_index(
                    conn,
                    id,
                    status == "queued" && !matches!(old.as_str(), "queued" | "running"),
                )?);
            }
            count += changed;
        }
        super::processing::refresh_groups(conn, keys)?;
        Ok(count)
    })
}

/// Attach a queue attempt before claiming legacy queued work that has no display record.
pub(crate) fn prepare_attempt(conn: &Connection, task: i64) -> AppResult<i64> {
    super::processing::atomic(conn, || {
        let current: Option<i64> = conn
            .query_row(
                "SELECT id FROM processing_tasks WHERE index_task_id=?",
                [task],
                |r| r.get(0),
            )
            .optional()?;
        if let Some(id) = current {
            return Ok(id);
        }
        let key = super::processing::sync_index(conn, task, true)?;
        super::processing::refresh_groups(conn, key)?;
        Ok(conn.query_row(
            "SELECT id FROM processing_tasks WHERE index_task_id=?",
            [task],
            |r| r.get(0),
        )?)
    })
}

pub fn cancel_asset(conn: &Connection, asset: i64, kind: &str) -> AppResult<()> {
    super::processing::atomic(conn, || {
        let mut q=conn.prepare("SELECT id FROM index_tasks WHERE asset_id=?1 AND kind=?2 AND status IN ('queued','running')")?;
        let ids = q
            .query_map(params![asset, kind], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            set_status(conn, Some(id), "cancelled", None, &["queued", "running"])?;
        }
        Ok(())
    })
}

pub fn fail_blocked_imports(conn: &Connection) -> AppResult<()> {
    super::processing::atomic(conn, || {
        let mut q=conn.prepare("SELECT id FROM index_tasks WHERE status='queued' AND EXISTS(SELECT 1 FROM assets a WHERE a.id=index_tasks.asset_id AND a.import_stage LIKE 'error%')")?;
        let ids = q
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        for id in ids {
            set_status(
                conn,
                Some(id),
                "failed",
                Some("Base import failed; retry importing this asset first"),
                &["queued"],
            )?;
        }
        Ok(())
    })
}

pub fn tasks(conn: &Connection) -> AppResult<Vec<IndexTask>> {
    tasks_for_asset(conn, None)
}
pub fn tasks_for_asset(conn: &Connection, asset: Option<i64>) -> AppResult<Vec<IndexTask>> {
    let mut q = conn.prepare("SELECT t.id,t.asset_id,a.file_name,t.kind,t.status,t.error FROM index_tasks t JOIN assets a ON a.id=t.asset_id WHERE (?1 IS NULL OR t.asset_id=?1) ORDER BY CASE t.status WHEN 'running' THEN 0 WHEN 'queued' THEN 1 WHEN 'failed' THEN 2 WHEN 'no_source' THEN 3 ELSE 4 END, t.id DESC LIMIT 200")?;
    let rows = q
        .query_map([asset], |r| {
            Ok(IndexTask {
                id: r.get(0)?,
                asset_id: r.get(1)?,
                file_name: r.get(2)?,
                kind: r.get(3)?,
                status: r.get(4)?,
                error: r.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}
