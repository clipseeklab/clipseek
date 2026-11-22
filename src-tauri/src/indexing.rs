//! Independent, persistent per-asset indexing queue. Base import never depends on it.
use crate::{
    db,
    error::{AppError, AppResult},
    library::LibrarySession,
};
use rusqlite::{params, OptionalExtension};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Duration,
};
use tauri::{Emitter, Manager};

fn emit<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session: &LibrarySession, id: i64) {
    if let Ok(Some(asset)) = db::assets::get_asset(&session.read_db(), &session.root, id) {
        crate::import::emit_asset_event(app, &session.meta.id, &asset);
    }
    let _ = app.emit("index://changed", &session.meta.id);
}

pub fn spawn_worker<R: tauri::Runtime>(app: &tauri::AppHandle<R>, session: Arc<LibrarySession>) {
    if session
        .index_worker_running
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        struct Guard(Arc<LibrarySession>, bool);
        impl Drop for Guard {
            fn drop(&mut self) {
                if self.1 {
                    self.0.index_worker_running.store(false, Ordering::SeqCst);
                }
            }
        }
        let mut worker_guard = Guard(session.clone(), true);
        let state = app.state::<crate::AppState>();
        while state
            .operation_running
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            std::thread::sleep(Duration::from_millis(200));
        }
        struct OperationGuard<'a>(&'a std::sync::atomic::AtomicBool);
        impl Drop for OperationGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, Ordering::SeqCst);
            }
        }
        let _operation = OperationGuard(&state.operation_running);
        loop {
            let permit = match crate::import::processing_permit::ProcessingPermit::claim(
                &state.import_ctl.running,
                &state.import_ctl.cancel,
            ) {
                Ok(permit) => permit,
                Err(_) => {
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            };
            let _activity = match session.asset_activity.work() {
                Ok(guard) => guard,
                Err(_) => {
                    drop(permit);
                    std::thread::sleep(Duration::from_millis(200));
                    continue;
                }
            };
            let task = {
                let conn = session.db.lock().unwrap();
                let _ = db::indexing::fail_blocked_imports(&conn);
                let next=conn.query_row("SELECT t.id,t.asset_id,t.kind FROM index_tasks t JOIN assets a ON a.id=t.asset_id WHERE t.status='queued' AND a.import_stage IN ('done','embedding') AND a.deleted_at IS NULL ORDER BY t.id LIMIT 1",[],|r|Ok((r.get::<_,i64>(0)?,r.get::<_,i64>(1)?,r.get::<_,String>(2)?))).optional()
                .map_err(AppError::from)
                .and_then(|next| next.map(|(id,asset,kind)|db::indexing::prepare_attempt(&conn,id).map(|attempt|(id,asset,kind,attempt))).transpose())
                ;
                if !matches!(&next, Ok(Some(_))) {
                    session.index_worker_running.store(false, Ordering::SeqCst);
                    worker_guard.1 = false;
                }
                next
            };
            let Ok(Some((task_id, asset_id, kind, attempt))) = task else {
                break;
            };
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                run_one(&app, &session, task_id, asset_id, &kind, attempt)
            }))
            .unwrap_or_else(|_| {
                Err(AppError::coded(
                    "index_build_failed",
                    "Index worker panicked",
                ))
            });
            if let Err(error) = result {
                let conn = session.db.lock().unwrap();
                let status = if error.code() == "subtitle_source_missing" {
                    "no_source"
                } else {
                    "failed"
                };
                let updated = db::indexing::set_status_if_current(
                    &conn,
                    task_id,
                    status,
                    Some(&error.to_string()),
                    &["running"],
                    attempt,
                )
                .unwrap_or(0);
                if updated > 0 {
                    let column = if kind == "visual" {
                        "visual_indexed"
                    } else {
                        "subtitle_indexed"
                    };
                    let _ = conn.execute(
                        &format!("UPDATE assets SET {column}=0 WHERE id=?"),
                        [asset_id],
                    );
                }
            }
            emit(&app, &session, asset_id);
            let last: bool = session
                .read_db()
                .query_row(
                    "SELECT NOT EXISTS(SELECT 1 FROM index_tasks WHERE status='queued')",
                    [],
                    |r| r.get(0),
                )
                .unwrap_or(false);
            if last {
                let _ = crate::vector::optimize_index(&session);
            }
            drop(permit);
        }
        let _ = app.emit("index://changed", &session.meta.id);
    });
}
fn fingerprint(path: &str) -> AppResult<(u64, std::time::SystemTime)> {
    let meta = std::fs::metadata(path)?;
    Ok((meta.len(), meta.modified()?))
}
fn run_one<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &Arc<LibrarySession>,
    task: i64,
    id: i64,
    kind: &str,
    attempt: i64,
) -> AppResult<()> {
    let _work = session.begin_asset_work(id)?;
    let asset = db::assets::get_asset(&session.read_db(), &session.root, id)?
        .filter(|a| a.deleted_at.is_none())
        .ok_or(AppError::AssetNotFound)?;
    {
        let conn = session.db.lock().unwrap();
        if db::indexing::set_status_if_current(&conn, task, "running", None, &["queued"], attempt)?
            == 0
        {
            return Ok(());
        }
    }
    emit(app, session, id);
    let before = fingerprint(&asset.abs_path)?;
    let result = if kind == "visual" {
        if asset.visual_indexed {
            Ok(())
        } else {
            // Only missing indexes are built. Existing valid generations are never cleared.
            (|| -> AppResult<()> {
                let candidates = asset
                    .duration_ms
                    .map(|ms| (ms.max(0) as u64 / 5000 + 1).min(3601))
                    .unwrap_or(1);
                let needed = 16 * 1024 * 1024 + candidates * 64 * 1024;
                let available = fs2::available_space(&session.root)?;
                if available < needed {
                    return Err(AppError::DiskSpaceInsufficient {
                        incoming_bytes: needed,
                        available_bytes: available,
                    });
                }
                let clip = app.state::<crate::AppState>().clip.get_or_init()?;
                crate::import::build_visual_index(session, id)?;
                crate::vector::embed_pending_while(session, &clip, &|_| {}, &|| {
                    session.read_db().query_row("SELECT EXISTS(SELECT 1 FROM index_tasks WHERE id=?1 AND status='running' AND EXISTS(SELECT 1 FROM processing_tasks p WHERE p.index_task_id=index_tasks.id AND p.id=?2))",params![task,attempt],|r|r.get::<_,bool>(0)).unwrap_or(false)
                })?;
                Ok(())
            })()
        }
    } else {
        if asset.subtitle_indexed {
            Ok(())
        } else {
            Err(AppError::InvalidArgument(
                "Only visual indexes are supported".into(),
            ))
        }
    };
    // Index errors cannot turn a usable media file into an import error.
    {
        let conn = session.db.lock().unwrap();
        db::assets::set_import_stage(&conn, id, "done")?;
    }
    result?;
    let changed = fingerprint(&asset.abs_path)? != before;
    publish_result(session, task, id, kind, before, changed, attempt)
}

fn publish_result(
    session: &LibrarySession,
    task: i64,
    id: i64,
    kind: &str,
    before: (u64, std::time::SystemTime),
    changed: bool,
    attempt: i64,
) -> AppResult<()> {
    let mut conn = session.db.lock().unwrap();
    let tx = conn.transaction()?;
    let active:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM index_tasks t JOIN assets a ON a.id=t.asset_id WHERE t.id=?1 AND t.status='running' AND a.deleted_at IS NULL AND EXISTS(SELECT 1 FROM processing_tasks p WHERE p.index_task_id=t.id AND p.id=?2))",params![task,attempt],|r|r.get(0))?;
    let column = if kind == "visual" {
        "visual_indexed"
    } else {
        "subtitle_indexed"
    };
    // A cancelled task may have been superseded by a new, valid subtitle import.
    // It must neither publish its result nor erase the replacement's readiness.
    if !active {
        tx.commit()?;
        return Ok(());
    }
    if changed {
        tx.execute(&format!("UPDATE assets SET {column}=0 WHERE id=?"), [id])?;
        db::indexing::set_status(
            &tx,
            Some(task),
            "failed",
            Some("Source changed during indexing"),
            &["running"],
        )?;
    } else {
        tx.execute(&format!("UPDATE assets SET {column}=1 WHERE id=?"), [id])?;
        let mtime = before
            .1
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        tx.execute("INSERT INTO index_versions(asset_id,kind,recipe,source_size,source_mtime) VALUES(?1,?2,'1',?3,?4) ON CONFLICT(asset_id,kind) DO UPDATE SET recipe=excluded.recipe,source_size=excluded.source_size,source_mtime=excluded.source_mtime",params![id,kind,before.0 as i64,mtime])?;
        db::indexing::set_status(&tx, Some(task), "completed", None, &["running"])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/index_commit.rs"]
mod commit_tests;
