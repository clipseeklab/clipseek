pub mod assets;
pub mod body;
pub mod diagnostics;
pub mod folders;
pub mod import;
pub mod indexing;
pub mod library;
pub mod model;
pub mod processing;
pub mod search;
pub mod settings;
pub mod tags;
#[path = "vector.rs"]
pub mod vector_recovery;

/// Execute only the current hover request in the host WebView's script context.
/// WKWebView's public evaluateJavaScript API supplies the media user gesture.
#[tauri::command]
pub fn hover_video_play(window: tauri::WebviewWindow, request: String) -> Result<(), String> {
    let request = serde_json::to_string(&request).map_err(|e| e.to_string())?;
    window.eval(format!(
        "document.querySelector('video[data-hover-player=shared]')?.dispatchEvent(new CustomEvent('host-hover-play', {{detail: {request}}}));"
    )).map_err(|e| e.to_string())
}

use tauri::{AppHandle, Manager};

use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::AppState;

/// Disk I/O, keychain calls and mutex waits must not run on the WebView thread.
pub async fn blocking<R: tauri::Runtime, T, F>(app: AppHandle<R>, work: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> AppResult<T> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || work(&app.state::<AppState>()))
        .await
        .map_err(|e| AppError::Msg(format!("background task failed: {e}")))?
}

/// Bind library-scoped work at the IPC boundary, including delayed commands.
pub async fn blocking_in_library<R: tauri::Runtime, T, F>(
    app: AppHandle<R>,
    context: body::LibraryContext,
    work: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppState, &LibrarySession) -> AppResult<T> + Send + 'static,
{
    let current = if context.0.is_none() {
        Some(crate::services::sessions::current(
            &app.state::<AppState>(),
        )?)
    } else {
        None
    };
    blocking(app, move |state| {
        let session = match current {
            Some(session) => session,
            None => crate::services::sessions::resolve(state, context.0.as_deref())?,
        };
        work(state, &session)
    })
    .await
}

/// Run against the current session or return LibraryNotOpen.
pub fn with_session<T>(
    state: &AppState,
    f: impl FnOnce(&LibrarySession) -> AppResult<T>,
) -> AppResult<T> {
    let guard = state.session.read().unwrap();
    match guard.as_deref() {
        Some(s) => f(s),
        None => Err(AppError::LibraryNotOpen),
    }
}
