//! HTTP transport, bearer tokens, network validation and server lifecycle.

use super::tools::ClipSeekMcp;
use super::{MCP_REQUESTS_PER_MINUTE, TOKEN_FILE};
use crate::error::{AppError, AppResult};
use crate::AppState;
use axum::{
    extract::{Request, State as AxumState},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::Response,
};
use rand::RngCore;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, RwLock},
};
use tauri::{AppHandle, Manager};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub(super) struct AuthState {
    pub(super) token: Arc<RwLock<String>>,
    pub(super) rate: Arc<Mutex<HashMap<String, (i64, u32)>>>,
}

#[derive(Clone)]
pub(super) struct NetworkState {
    pub(super) hosts: Arc<Vec<String>>,
    pub(super) origins: Arc<Vec<String>>,
    pub(super) lan_enabled: bool,
}

fn is_local_network_address(ip: std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(ip) => ip.is_loopback() || ip.is_private() || ip.is_link_local(),
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback() || ip.is_unique_local() || ip.is_unicast_link_local()
        }
    }
}

pub(super) async fn validate_network_headers(
    AxumState(network): AxumState<NetworkState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let remote_ip = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|connect| connect.0.ip())
        .ok_or(StatusCode::FORBIDDEN)?;
    if !is_local_network_address(remote_ip) || (!network.lan_enabled && !remote_ip.is_loopback()) {
        return Err(StatusCode::FORBIDDEN);
    }
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(StatusCode::BAD_REQUEST)?;
    let host_name = host
        .strip_prefix('[')
        .and_then(|value| value.split_once(']').map(|(name, _)| name))
        .or_else(|| host.split(':').next())
        .unwrap_or(host);
    if !network
        .hosts
        .iter()
        .any(|allowed| allowed.eq_ignore_ascii_case(host_name))
    {
        return Err(StatusCode::FORBIDDEN);
    }
    if let Some(origin) = request
        .headers()
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    {
        if !network.origins.iter().any(|allowed| allowed == origin) {
            return Err(StatusCode::FORBIDDEN);
        }
    }
    Ok(next.run(request).await)
}

pub(super) async fn authorize(
    AxumState(auth): AxumState<AuthState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let expected = auth.token.read().unwrap().clone();
    let provided = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "));
    if expected.is_empty() || provided != Some(expected.as_str()) {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let remote = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|value| value.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    let minute = crate::util::now() / 60;
    {
        let mut rate = auth.rate.lock().unwrap();
        let entry = rate.entry(remote).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        if entry.1 >= MCP_REQUESTS_PER_MINUTE {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        entry.1 += 1;
    }
    Ok(next.run(request).await)
}

pub(super) fn token_path(state: &AppState) -> AppResult<PathBuf> {
    let operations = state.config.read().unwrap().operations_dir()?;
    Ok(operations.parent().unwrap().join(TOKEN_FILE))
}

fn load_or_create_token(state: &AppState) -> AppResult<String> {
    let path = token_path(state)?;
    if let Ok(value) = fs::read_to_string(&path) {
        let value = value.trim();
        if value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()) {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            }
            return Ok(value.to_string());
        }
    }
    rotate_token_file(state)
}

fn rotate_token_file(state: &AppState) -> AppResult<String> {
    let path = token_path(state)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let token = hex::encode(bytes);
    let temp = path.with_extension("tmp");
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .mode(0o600)
            .open(&temp)?;
        file.write_all(token.as_bytes())?;
    }
    #[cfg(not(unix))]
    fs::write(&temp, token.as_bytes())?;
    fs::rename(temp, path)?;
    Ok(token)
}

pub fn reveal_token(state: &AppState) -> AppResult<String> {
    let _token_guard = state.mcp.token_file_lock.lock().unwrap();
    let token = load_or_create_token(state)?;
    *state.mcp.token.write().unwrap() = token.clone();
    Ok(token)
}

pub fn rotate_token(state: &AppState) -> AppResult<String> {
    let _token_guard = state.mcp.token_file_lock.lock().unwrap();
    let token = rotate_token_file(state)?;
    *state.mcp.token.write().unwrap() = token.clone();
    Ok(token)
}

pub(super) fn primary_lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind((std::net::Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket
        .connect((std::net::Ipv4Addr::new(192, 0, 2, 1), 80))
        .ok()?;
    let ip = socket.local_addr().ok()?.ip();
    (!ip.is_loopback() && !ip.is_unspecified()).then_some(ip)
}

pub(super) fn service_base_url(settings: &crate::config::Settings, port: u16) -> Option<String> {
    if settings.mcp_listen_mode == "lan" {
        return Some(
            primary_lan_ip()
                .map(|ip| format!("http://{ip}:{port}"))
                .unwrap_or_else(|| format!("http://127.0.0.1:{port}")),
        );
    }
    Some(format!("http://127.0.0.1:{port}"))
}

pub async fn apply_settings<R: tauri::Runtime>(app: &AppHandle<R>) -> AppResult<u16> {
    let state = app.state::<AppState>();
    let _settings_guard = state.mcp.settings_lock.lock().await;
    let settings = state.config.read().unwrap().settings.clone();
    if !settings.mcp_enabled {
        state.mcp.stop();
        return Ok(settings.mcp_port);
    }
    let port = start(app.clone(), settings.mcp_port).await?;
    if settings.mcp_port != port {
        let saved = (|| -> AppResult<()> {
            let mut guard = state.config.write().unwrap();
            let mut next = guard.clone();
            next.settings.mcp_port = port;
            next.save()?;
            *guard = next;
            Ok(())
        })();
        if let Err(error) = saved {
            state.mcp.stop();
            return Err(error);
        }
    }
    Ok(port)
}

async fn start<R: tauri::Runtime>(app: AppHandle<R>, preferred_port: u16) -> AppResult<u16> {
    let state = app.state::<AppState>();
    let settings = state.config.read().unwrap().settings.clone();
    state.mcp.stop();
    if let Err(error) = reveal_token(&state) {
        state.mcp.set_error(error.to_string());
        return Err(error);
    }

    let bind_ip = if settings.mcp_listen_mode == "lan" {
        "0.0.0.0"
    } else {
        "127.0.0.1"
    };
    let listener = match tokio::net::TcpListener::bind((bind_ip, preferred_port)).await {
        Ok(listener) => listener,
        Err(first) if preferred_port == 0 => {
            state.mcp.set_error(first.to_string());
            return Err(AppError::coded(
                "mcp_listen_failed",
                format!("MCP listen failed: {first}"),
            ));
        }
        Err(error) => {
            state
                .mcp
                .set_error(format!("Port {preferred_port} is occupied: {error}"));
            return Err(AppError::coded(
                "mcp_port_occupied",
                format!("MCP port {preferred_port} is occupied"),
            ));
        }
    };
    let port = listener.local_addr()?.port();
    let mut allowed_hosts = vec![
        "localhost".to_string(),
        "127.0.0.1".to_string(),
        "::1".to_string(),
    ];
    if settings.mcp_listen_mode == "lan" {
        if let Some(ip) = primary_lan_ip() {
            allowed_hosts.push(ip.to_string());
        }
    }
    let mut allowed_origins = vec![
        format!("http://localhost:{port}"),
        format!("http://127.0.0.1:{port}"),
        format!("http://[::1]:{port}"),
    ];
    if let Some(ip) = primary_lan_ip().filter(|_| settings.mcp_listen_mode == "lan") {
        allowed_origins.push(format!("http://{ip}:{port}"));
    }
    let cancellation = CancellationToken::new();
    let server_app = app.clone();
    let service: StreamableHttpService<ClipSeekMcp<R>, LocalSessionManager> =
        StreamableHttpService::new(
            move || Ok(ClipSeekMcp::new(server_app.clone())),
            Default::default(),
            StreamableHttpServerConfig::default()
                .with_legacy_session_mode(false)
                .with_json_response(true)
                .with_cancellation_token(cancellation.clone())
                .with_allowed_hosts(allowed_hosts.clone())
                .with_allowed_origins(allowed_origins.clone())
                .with_max_request_body_bytes(1024 * 1024),
        );
    let auth = AuthState {
        token: state.mcp.token.clone(),
        rate: state.mcp.auth_rate.clone(),
    };
    let mcp_router = axum::Router::new()
        .nest_service("/mcp", service)
        .layer(middleware::from_fn_with_state(auth, authorize));
    let network = NetworkState {
        hosts: Arc::new(allowed_hosts),
        origins: Arc::new(allowed_origins),
        lan_enabled: settings.mcp_listen_mode == "lan",
    };
    let router = mcp_router.layer(middleware::from_fn_with_state(
        network,
        validate_network_headers,
    ));
    let generation = {
        let mut runtime = state.mcp.state.lock().unwrap();
        runtime.generation = runtime.generation.wrapping_add(1);
        runtime.running = true;
        runtime.port = port;
        runtime.error = None;
        runtime.cancellation = Some(cancellation.clone());
        runtime.generation
    };
    let weak_app = app.clone();
    tauri::async_runtime::spawn(async move {
        let result = axum::serve(
            listener,
            router.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .with_graceful_shutdown(cancellation.cancelled_owned())
        .await;
        let state = weak_app.state::<AppState>();
        let mut runtime = state.mcp.state.lock().unwrap();
        if runtime.generation == generation {
            runtime.running = false;
            if let Err(error) = result {
                runtime.error = Some(error.to_string());
            }
        }
    });
    Ok(port)
}
