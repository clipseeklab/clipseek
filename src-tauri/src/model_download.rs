use crate::{
    embedding::{
        domain::{Precision, Settings, MODEL_REVISION},
        models,
    },
    error::{AppError, AppResult},
    AppState,
};
use std::{
    path::PathBuf,
    sync::{Mutex, OnceLock, RwLock},
};
use tauri::{Emitter, Manager};
pub(crate) static SETTINGS: OnceLock<RwLock<Settings>> = OnceLock::new();
static RUNTIME: OnceLock<PathBuf> = OnceLock::new();
pub(crate) static DOWNLOAD: Mutex<()> = Mutex::new(());
pub fn runtime() -> PathBuf {
    RUNTIME
        .get()
        .cloned()
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime"))
}
pub fn settings() -> Settings {
    SETTINGS
        .get_or_init(|| {
            RwLock::new(Settings {
                precision: Precision::Q8,
                model_directory: std::env::var("CLIPSTUDIO_MODELS").unwrap_or_else(|_| {
                    runtime().join("../.local/models").to_string_lossy().into()
                }),
            })
        })
        .read()
        .unwrap()
        .clone()
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub ready: bool,
    pub execution: Option<crate::embedding::execution::ExecutionStatus>,
    pub version: &'static str,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub precision: Precision,
    pub model_directory: String,
}
pub fn configure(app: &tauri::AppHandle) -> AppResult<PathBuf> {
    let bundled = app
        .path()
        .resource_dir()
        .map_err(|e| AppError::Msg(e.to_string()))?
        .join("runtime");
    let dir = if bundled.join("versions.json").exists() {
        bundled
    } else {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../runtime")
    };
    let _ = RUNTIME.set(dir);
    let cfg = app
        .state::<AppState>()
        .config
        .read()
        .unwrap()
        .settings
        .clone();
    let root = if !cfg.embedding_model_directory.is_empty() {
        cfg.embedding_model_directory
    } else if let Ok(path) = std::env::var("CLIPSTUDIO_MODELS") {
        path
    } else if runtime()
        .parent()
        .is_some_and(|p| p.join(".local/models/q8").exists())
    {
        runtime()
            .parent()
            .unwrap()
            .join(".local/models")
            .to_string_lossy()
            .into()
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| AppError::Msg(e.to_string()))?
            .join("models")
            .to_string_lossy()
            .into()
    };
    *SETTINGS
        .get_or_init(|| {
            RwLock::new(Settings {
                precision: cfg.embedding_precision,
                model_directory: root.clone(),
            })
        })
        .write()
        .unwrap() = Settings {
        precision: cfg.embedding_precision,
        model_directory: root.clone(),
    };
    Ok(PathBuf::from(root))
}
pub(crate) fn status(state: &AppState) -> ModelStatus {
    let s = settings();
    ModelStatus {
        execution: state.clip.execution_status(),
        ready: models::available(&s),
        version: MODEL_REVISION,
        downloaded_bytes: 0,
        total_bytes: 0,
        precision: s.precision,
        model_directory: s.model_directory,
    }
}
#[tauri::command]
pub async fn model_download(app: tauri::AppHandle) -> AppResult<ModelStatus> {
    let cloned = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = DOWNLOAD.lock().unwrap();
        models::download(&settings(), &|progress| {
            let _ = cloned.emit(
                "model://progress",
                serde_json::json!({
                    "downloadedBytes": progress.bytes, "totalBytes": progress.total,
                    "currentFile": progress.file, "speedBps": 0, "etaSeconds": 0
                }),
            );
        })
        .map_err(|error| AppError::Msg(format!("{error:#}")))?;
        Ok::<_, AppError>(())
    })
    .await
    .map_err(|error| AppError::Msg(error.to_string()))??;
    let state = app.state::<AppState>();
    let libraries = state.config.read().unwrap().libraries.clone();
    for library in libraries {
        match crate::services::sessions::resolve(&state, Some(&library.id)) {
            Ok(session) => crate::services::maintenance::spawn_library_maintenance(&app, session),
            Err(error) => crate::logging::warn(
                "model",
                "resume_library_failed",
                &library.id,
                &error.to_string(),
            ),
        }
    }
    Ok(status(&state))
}
