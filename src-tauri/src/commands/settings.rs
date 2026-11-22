use tauri::AppHandle;

use crate::config::{OnboardingState, Settings, ONBOARDING_VERSION};
use crate::error::{AppError, AppResult};
use crate::AppState;

/// Read user settings.
#[tauri::command]
pub async fn settings_get<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<Settings> {
    crate::commands::blocking(app, settings_get_blocking).await
}

pub(crate) fn settings_get_blocking(state: &AppState) -> AppResult<Settings> {
    let mut cfg = state.config.write().unwrap();
    cfg.ensure_loaded()?;
    let result = cfg.settings.clone();
    Ok(result)
}

#[tauri::command]
pub fn tray_set_labels(app: AppHandle, show: String, quit: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use tauri::menu::MenuBuilder;

        if let Some(tray) = app.tray_by_id("main") {
            let menu = MenuBuilder::new(&app)
                .text("show", show)
                .separator()
                .text("quit", quit)
                .build()
                .map_err(|error| error.to_string())?;
            tray.set_menu(Some(menu))
                .map_err(|error| error.to_string())?;
        }
    }
    #[cfg(not(target_os = "windows"))]
    let _ = (app, show, quit);
    Ok(())
}

#[tauri::command]
pub async fn onboarding_get<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<OnboardingState> {
    crate::commands::blocking(app, onboarding_get_blocking).await
}

pub(crate) fn onboarding_get_blocking(state: &AppState) -> AppResult<OnboardingState> {
    let mut cfg = state.config.write().unwrap();
    cfg.ensure_loaded()?;
    Ok(cfg.onboarding.clone())
}

#[tauri::command]
pub async fn onboarding_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    current_step: u8,
    completed: bool,
) -> AppResult<OnboardingState> {
    crate::commands::blocking(app, move |state| {
        onboarding_set_blocking(state, current_step, completed)
    })
    .await
}

pub(crate) fn onboarding_set_blocking(
    state: &AppState,
    current_step: u8,
    completed: bool,
) -> AppResult<OnboardingState> {
    let mut saved = state.config.write().unwrap();
    saved.ensure_loaded()?;
    let mut cfg = saved.clone();
    cfg.onboarding = OnboardingState {
        version: ONBOARDING_VERSION,
        current_step: current_step.clamp(1, 3),
        completed,
    };
    cfg.save()?;
    *saved = cfg;
    Ok(saved.onboarding.clone())
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsPatch {
    mcp_enabled: Option<bool>,
    mcp_write_enabled: Option<bool>,
    mcp_listen_mode: Option<String>,
    language: Option<String>,
    theme: Option<String>,
    trash_retention_days: Option<u32>,
    import_mode: Option<String>,
    auto_visual_index: Option<bool>,
}
#[tauri::command]
pub async fn settings_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    patch: super::body::CommandBody<SettingsPatch>,
) -> AppResult<Settings> {
    let patch = patch.0;
    let restart_mcp = patch.mcp_enabled.is_some() || patch.mcp_listen_mode.is_some();
    crate::commands::blocking(app.clone(), move |state| {
        if patch
            .import_mode
            .as_deref()
            .is_some_and(|v| v != "reference")
        {
            return Err(AppError::InvalidArgument(
                "Only reference imports are supported".into(),
            ));
        }
        if patch
            .mcp_listen_mode
            .as_deref()
            .is_some_and(|mode| !matches!(mode, "loopback" | "lan"))
        {
            return Err(AppError::InvalidArgument("Invalid MCP listen mode".into()));
        }
        let mut guard = state.config.write().unwrap();
        guard.ensure_loaded()?;
        let mut cfg = guard.clone();
        if let Some(value) = patch.language {
            cfg.settings.language = value;
        }
        if let Some(value) = patch.theme {
            cfg.settings.theme = value;
        }
        if let Some(value) = patch.trash_retention_days {
            cfg.settings.trash_retention_days = value.clamp(1, 365);
        }
        if let Some(value) = patch.auto_visual_index {
            cfg.settings.auto_visual_index = value;
        }
        if let Some(value) = patch.mcp_enabled {
            cfg.settings.mcp_enabled = value;
            if !value {
                cfg.settings.mcp_write_enabled = false;
            }
        }
        if let Some(value) = patch.mcp_write_enabled {
            cfg.settings.mcp_write_enabled = value && cfg.settings.mcp_enabled;
        }
        if let Some(value) = patch.mcp_listen_mode {
            cfg.settings.mcp_listen_mode = value;
        }
        cfg.settings.import_mode = "reference".into();
        cfg.settings.auto_subtitle_index = false;
        cfg.save()?;
        *guard = cfg;
        Ok(guard.settings.clone())
    })
    .await?;
    if restart_mcp {
        if let Err(error) = crate::mcp::apply_settings(&app).await {
            crate::logging::error("mcp", "start_failed", "-", &error.to_string());
        }
    }
    settings_get(app).await
}

#[tauri::command]
pub async fn mcp_status<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<crate::mcp::McpStatus> {
    crate::commands::blocking(app, move |state| Ok(mcp_status_blocking(state))).await
}

pub(crate) fn mcp_status_blocking(state: &AppState) -> crate::mcp::McpStatus {
    state.mcp.status(&state.config.read().unwrap().settings)
}

#[tauri::command]
pub async fn mcp_token_reveal<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<String> {
    crate::commands::blocking(app, mcp_token_reveal_blocking).await
}

pub(crate) fn mcp_token_reveal_blocking(state: &AppState) -> AppResult<String> {
    crate::mcp::reveal_token(state)
}

#[tauri::command]
pub async fn mcp_token_rotate<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<String> {
    crate::commands::blocking(app, mcp_token_rotate_blocking).await
}

pub(crate) fn mcp_token_rotate_blocking(state: &AppState) -> AppResult<String> {
    crate::mcp::rotate_token(state)
}
