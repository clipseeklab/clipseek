//! Authenticated local/LAN MCP runtime and shared operation state.

use crate::error::{AppError, AppResult};
use crate::AppState;
use serde::Serialize;
use serde_json::json;
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::Write,
    sync::{Arc, Mutex, RwLock},
};
use tokio_util::sync::CancellationToken;

mod import;
mod resources;
mod server;
mod tools;

pub use server::{apply_settings, reveal_token, rotate_token};
use server::{primary_lan_ip, service_base_url};

const MAX_PAGE_SIZE: u32 = 50;
const TOKEN_FILE: &str = "mcp-token";
const AUDIT_FILE: &str = "mcp-audit.jsonl";
const MCP_REQUESTS_PER_MINUTE: u32 = 240;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpStatus {
    pub enabled: bool,
    pub running: bool,
    pub write_enabled: bool,
    pub port: u16,
    pub url: Option<String>,
    pub local_url: Option<String>,
    pub lan_url: Option<String>,
    pub listen_mode: String,
    pub error: Option<String>,
}

#[derive(Default)]
struct RuntimeState {
    running: bool,
    port: u16,
    error: Option<String>,
    cancellation: Option<CancellationToken>,
    generation: u64,
}

#[derive(Default)]
pub struct McpRuntime {
    state: Mutex<RuntimeState>,
    token_file_lock: Mutex<()>,
    settings_lock: tokio::sync::Mutex<()>,
    token: Arc<RwLock<String>>,
    auth_rate: Arc<Mutex<HashMap<String, (i64, u32)>>>,
}

impl McpRuntime {
    pub fn status(&self, settings: &crate::config::Settings) -> McpStatus {
        let state = self.state.lock().unwrap();
        McpStatus {
            enabled: settings.mcp_enabled,
            running: state.running,
            write_enabled: settings.mcp_write_enabled,
            port: if state.port == 0 {
                settings.mcp_port
            } else {
                state.port
            },
            url: (state.running && state.port != 0)
                .then(|| service_base_url(settings, state.port).map(|base| format!("{base}/mcp")))
                .flatten(),
            local_url: (state.running && state.port != 0)
                .then(|| format!("http://127.0.0.1:{}/mcp", state.port)),
            lan_url: (state.running && state.port != 0 && settings.mcp_listen_mode == "lan")
                .then(|| primary_lan_ip().map(|ip| format!("http://{ip}:{}/mcp", state.port)))
                .flatten(),
            listen_mode: settings.mcp_listen_mode.clone(),
            error: state.error.clone(),
        }
    }

    pub fn stop(&self) {
        let mut state = self.state.lock().unwrap();
        if let Some(token) = state.cancellation.take() {
            token.cancel();
        }
        state.running = false;
        state.error = None;
    }

    fn set_error(&self, error: impl Into<String>) {
        let mut state = self.state.lock().unwrap();
        state.running = false;
        state.error = Some(error.into());
        state.cancellation = None;
    }
}

fn validate_uid(uid: &str) -> AppResult<()> {
    uuid::Uuid::parse_str(uid)
        .map(|_| ())
        .map_err(|_| AppError::InvalidArgument("invalid UID".into()))
}

fn ensure_write(state: &AppState) -> AppResult<()> {
    let settings = &state.config.read().unwrap().settings;
    if !settings.mcp_enabled {
        return Err(AppError::McpDisabled);
    }
    if !settings.mcp_write_enabled {
        return Err(AppError::McpWriteDisabled);
    }
    Ok(())
}

fn audit(
    state: &AppState,
    tool: &str,
    library_id: &str,
    asset_uids: &[String],
    success: bool,
    item_count: Option<usize>,
) {
    let Ok(dir) = server::token_path(state).map(|p| p.parent().unwrap().to_path_buf()) else {
        return;
    };
    let _ = fs::create_dir_all(&dir);
    let entry = json!({"timestamp": crate::util::now(), "client": "mcp", "tool": tool, "libraryId": library_id, "assetUids": asset_uids, "itemCount": item_count, "success": success});
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join(AUDIT_FILE))
    {
        let _ = writeln!(file, "{entry}");
    }
}

#[cfg(test)]
#[path = "../tests/unit/mcp.rs"]
mod tests;
