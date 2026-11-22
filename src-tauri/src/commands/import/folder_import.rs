//! Directory import IPC adapter.
use crate::error::AppResult;
use crate::services::imports::{self, ImportAccepted};

/// Create matching logical folders for dropped physical directories and import their
/// immediate children. Registration is concurrent; processing remains serialized.
#[tauri::command]
pub async fn import_folders(
    app: tauri::AppHandle,
    paths: Vec<String>,
    batch_id: Option<String>,
) -> AppResult<ImportAccepted> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        imports::submit_folders(worker_app, state, paths, batch_id)
    })
    .await
}
