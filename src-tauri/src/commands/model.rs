//! Model preferences use the same blocking command boundary as other settings.
use crate::embedding::domain::{Precision, Settings};
use crate::error::{AppError, AppResult};
use crate::model_download::{status, ModelStatus, DOWNLOAD, SETTINGS};
#[tauri::command]
pub async fn model_status<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<ModelStatus> {
    crate::commands::blocking(app, |state| Ok(status(state))).await
}

#[tauri::command]
pub async fn model_settings<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    precision: Precision,
    model_directory: String,
) -> AppResult<ModelStatus> {
    let worker = app.clone();
    crate::commands::blocking(app, move |state| {
        let _settings = crate::model_download::settings();
        let _download = DOWNLOAD.try_lock().map_err(|_| AppError::OperationBusy)?;
        if !std::path::Path::new(&model_directory).is_absolute() {
            return Err(AppError::InvalidArgument(
                "Model directory must be absolute".into(),
            ));
        }
        if state
            .import_ctl
            .running
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(AppError::OperationBusy);
        }
        let metas = state.config.read().unwrap().libraries.clone();
        let mut sessions = Vec::new();
        for meta in metas {
            let s = crate::services::sessions::resolve(state, Some(&meta.id))?;
            if s.index_worker_running
                .load(std::sync::atomic::Ordering::Acquire)
            {
                return Err(AppError::OperationBusy);
            }
            sessions.push(s);
        }
        let guards = sessions
            .iter()
            .map(|s| s.asset_activity.cleanup())
            .collect::<AppResult<Vec<_>>>()?;
        for session in &sessions {
            let conn = session.db.lock().unwrap();
            conn.execute("UPDATE frames SET embedded=0", [])?;
            conn.execute(
                "UPDATE assets SET visual_indexed=0 WHERE deleted_at IS NULL",
                [],
            )?;
            conn.execute(
                "UPDATE index_tasks SET status='cancelled' WHERE status IN ('queued','running')",
                [],
            )?;
            let mut ids = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NULL")?;
            let ids = ids
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            for id in ids {
                crate::db::indexing::enqueue(&conn, id, "visual")?;
            }
        }
        {
            let mut cfg = state.config.write().unwrap();
            let mut next = cfg.clone();
            next.settings.embedding_precision = precision;
            next.settings.embedding_model_directory = model_directory.clone();
            next.save()?;
            *cfg = next;
        }
        *SETTINGS.get().unwrap().write().unwrap() = Settings {
            precision,
            model_directory,
        };
        state.clip.invalidate();
        drop(guards);
        if crate::embedding::models::available(&crate::model_download::settings()) {
            for session in sessions {
                crate::services::maintenance::spawn_library_maintenance(&worker, session);
            }
        }
        Ok(status(state))
    })
    .await
}

#[cfg(test)]
#[path = "../../tests/unit/model_settings.rs"]
mod tests;
