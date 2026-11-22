//! Queue queries and explicit user actions; all scoped to the originating library.
use crate::{
    db,
    error::{AppError, AppResult},
};
use tauri::Emitter;

#[tauri::command]
pub async fn processing_summary<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
) -> AppResult<db::processing::Summary> {
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        // Cleanup never waits behind an import transaction just to refresh a badge.
        if let Ok(conn) = session.db.try_lock() {
            db::processing::maintain(&conn, crate::util::now())?;
        }
        let result = db::processing::summary(&session.read_db());
        result
    })
    .await
}

#[tauri::command]
pub async fn processing_history<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
    key: String,
    before: Option<i64>,
) -> AppResult<db::processing::HistoryPage> {
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        let result = db::processing::history(&session.read_db(), &key, before);
        result
    })
    .await
}

#[tauri::command]
pub async fn processing_list<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
    filter: db::processing::Filter,
) -> AppResult<db::processing::Page> {
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        let result = db::processing::list(&session.read_db(), &filter);
        result
    })
    .await
}

#[tauri::command]
pub async fn processing_clear<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
) -> AppResult<i64> {
    let worker = app.clone();
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        let count = db::processing::clear_finished(&session.db.lock().unwrap())?;
        let _ = worker.emit("processing://changed", library_id);
        Ok(count)
    })
    .await
}

#[tauri::command]
pub async fn processing_hide<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
    key: String,
) -> AppResult<()> {
    let worker = app.clone();
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        db::processing::hide(&session.db.lock().unwrap(), &key)?;
        let _ = worker.emit("processing://changed", library_id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn processing_cancel<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
    id: i64,
) -> AppResult<()> {
    let worker = app.clone();
    super::blocking(app, move |state| {
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        let conn = session.db.lock().unwrap();
        let target = db::processing::get(&conn, id)?.ok_or(AppError::AssetNotFound)?;
        let task = target.index_task_id.ok_or_else(|| {
            AppError::InvalidArgument("This task cannot be cancelled individually".into())
        })?;
        db::indexing::set_status(&conn, Some(task), "cancelled", None, &["queued", "running"])?;
        drop(conn);
        let _ = worker.emit("index://changed", library_id);
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn processing_retry(app: tauri::AppHandle, library_id: String, id: i64) -> AppResult<()> {
    let worker = app.clone();
    super::blocking(app, move |state| {
        let _switch = state.session_switch_lock.lock().unwrap();
        let session = crate::services::sessions::current(state)?;
        if session.meta.id != library_id {
            return Err(AppError::InvalidArgument("Library changed".into()));
        }
        let db::processing::ActionTarget {
            asset_id: asset,
            source_path: path,
            kind,
            status,
            index_task_id: index,
            batch_id: batch,
            target_folder_id: folder,
        } = db::processing::get(&session.read_db(), id)?.ok_or(AppError::AssetNotFound)?;
        if !matches!(
            status.as_str(),
            "failed" | "interrupted" | "cancelled" | "no_source"
        ) {
            return Err(AppError::InvalidArgument(
                "Only unfinished tasks can be retried".into(),
            ));
        }
        if let Some(task) = index {
            let conn = session.db.lock().unwrap();
            let changed = db::indexing::set_status(
                &conn,
                Some(task),
                "queued",
                None,
                &["failed", "cancelled", "no_source"],
            )?;
            if changed == 0 {
                return Err(AppError::OperationBusy);
            }
            drop(conn);
            crate::indexing::spawn_worker(&worker, session);
        } else if kind == "import" {
            if let Some(asset) = asset {
                let result =
                    super::import::import_retry_blocking(worker.clone(), state, Some(vec![asset]))?;
                if result.failed > 0 {
                    return Err(AppError::coded(
                        "import_retry_failed",
                        "Import retry failed; see the task details",
                    ));
                }
            } else {
                crate::services::imports::submit(
                    worker.clone(),
                    state,
                    crate::services::imports::PathImport {
                        paths: vec![path],
                        folder_id: folder,
                        batch_id: Some(batch),
                        ..Default::default()
                    },
                )?;
            }
        } else {
            return Err(AppError::InvalidArgument(
                "Resolve this notice from its authorization dialog".into(),
            ));
        }
        let _ = worker.emit("processing://changed", library_id);
        Ok(())
    })
    .await
}
