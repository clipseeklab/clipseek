//! Tauri adapters for the persistent indexing queue.
use crate::{
    db::{
        self,
        indexing::{IndexScope, IndexTask},
    },
    error::{AppError, AppResult},
    indexing::spawn_worker,
};
use std::sync::atomic::Ordering;
use tauri::Emitter;

#[tauri::command]
pub async fn index_build<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    scope: IndexScope,
    kinds: Vec<String>,
) -> AppResult<usize> {
    let worker = app.clone();
    crate::commands::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if state.operation_running.load(Ordering::SeqCst)
            && !session.index_worker_running.load(Ordering::SeqCst)
        {
            return Err(AppError::OperationBusy);
        }
        let count = {
            let mut conn = session.db.lock().unwrap();
            let tx = conn.transaction()?;
            let n = db::indexing::enqueue_scope(&tx, &scope, &kinds)?;
            tx.commit()?;
            n
        };
        let _ = worker.emit("processing://changed", &session.meta.id);
        spawn_worker(&worker, session);
        Ok(count)
    })
    .await
}
#[tauri::command]
pub async fn index_tasks<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    asset_id: Option<i64>,
) -> AppResult<Vec<IndexTask>> {
    crate::commands::blocking(app, move |state| {
        crate::commands::with_session(state, |s| {
            db::indexing::tasks_for_asset(&s.read_db(), asset_id)
        })
    })
    .await
}
#[tauri::command]
pub async fn index_task_cancel<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: Option<i64>,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        crate::commands::with_session(state, |s| {
            db::indexing::set_status(
                &s.db.lock().unwrap(),
                id,
                "cancelled",
                None,
                &["queued", "running"],
            )?;
            Ok(())
        })
    })
    .await
}
#[tauri::command]
pub async fn index_task_retry<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
) -> AppResult<()> {
    let worker = app.clone();
    crate::commands::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        {
            let conn = session.db.lock().unwrap();
            db::indexing::set_status(
                &conn,
                Some(id),
                "queued",
                None,
                &["failed", "cancelled", "no_source"],
            )?;
        }
        spawn_worker(&worker, session);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn index_summary<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<serde_json::Value> {
    crate::commands::blocking(app,|state|crate::commands::with_session(state,|s|{
        let conn=s.read_db();
        let (assets,visual,subtitles):(i64,i64,i64)=conn.query_row("SELECT COUNT(*),COALESCE(SUM(visual_indexed),0),COALESCE(SUM(subtitle_indexed),0) FROM assets WHERE deleted_at IS NULL",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
        let mut q=conn.prepare("SELECT status,COUNT(*) FROM index_tasks GROUP BY status")?;
        let rows=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,i64>(1)?)))?.collect::<Result<Vec<_>,_>>()?;
        let counts:std::collections::HashMap<_,_>=rows.into_iter().collect();
        Ok(serde_json::json!({"assets":assets,"visual":visual,"subtitles":subtitles,"tasks":counts,"visualModelReady":state.clip.visual_ready()}))
    })).await
}
