//! IPC adapters; domain behavior stays outside the command boundary.

use crate::{diagnostics, error::AppResult};

#[tauri::command]
pub async fn diagnostics_open_log_dir<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<()> {
    crate::commands::blocking(app, move |_state| {
        diagnostics::diagnostics_open_log_dir_blocking()
    })
    .await
}

#[tauri::command]
pub async fn diagnostics_export<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    target_path: String,
) -> AppResult<String> {
    crate::commands::blocking(app, move |state| {
        diagnostics::diagnostics_export_blocking(state, target_path)
    })
    .await
}

#[tauri::command]
pub fn frontend_error(kind: String, message: String) {
    crate::logging::error("frontend", &kind, "-", &format!("message={message}"));
}
