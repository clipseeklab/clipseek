use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use std::{
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

struct PlaybackHttpServer {
    base_url: String,
    downloads: Arc<crate::playback_media::DownloadRegistry>,
}

static PLAYBACK_HTTP_SERVER: OnceLock<PlaybackHttpServer> = OnceLock::new();
static PLAYBACK_HTTP_START_LOCK: Mutex<()> = Mutex::new(());

fn playback_http_server() -> AppResult<&'static PlaybackHttpServer> {
    if let Some(server) = PLAYBACK_HTTP_SERVER.get() {
        return Ok(server);
    }
    let _guard = PLAYBACK_HTTP_START_LOCK.lock().unwrap();
    if let Some(server) = PLAYBACK_HTTP_SERVER.get() {
        return Ok(server);
    }
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    listener.set_nonblocking(true)?;
    let downloads = Arc::new(crate::playback_media::DownloadRegistry::for_local_playback());
    let route_downloads = downloads.clone();
    std::thread::Builder::new()
        .name("clipseek-playback-http".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    crate::logging::error(
                        "playback",
                        "http_runtime_failed",
                        "-",
                        &error.to_string(),
                    );
                    return;
                }
            };
            runtime.block_on(async move {
                let listener = match tokio::net::TcpListener::from_std(listener) {
                    Ok(listener) => listener,
                    Err(error) => {
                        crate::logging::error(
                            "playback",
                            "http_listener_failed",
                            "-",
                            &error.to_string(),
                        );
                        return;
                    }
                };
                let router = axum::Router::new()
                    .route(
                        "/files/{download_id}/{file_name}",
                        axum::routing::get(crate::playback_media::download)
                            .head(crate::playback_media::download),
                    )
                    .with_state(route_downloads);
                if let Err(error) = axum::serve(listener, router).await {
                    crate::logging::error(
                        "playback",
                        "http_server_failed",
                        "-",
                        &error.to_string(),
                    );
                }
            });
        })?;
    let server = PlaybackHttpServer {
        base_url: format!("http://127.0.0.1:{port}"),
        downloads,
    };
    let _ = PLAYBACK_HTTP_SERVER.set(server);
    PLAYBACK_HTTP_SERVER
        .get()
        .ok_or_else(|| AppError::coded("playback_server_failed", "playback server did not start"))
}

#[cfg(test)]
fn playback_http_url(path: &Path, mime_type: &str) -> AppResult<String> {
    let server = playback_http_server()?;
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("video.mp4")
        .to_string();
    Ok(server
        .downloads
        .issue_inline(
            path.to_path_buf(),
            file_name,
            mime_type.to_string(),
            &server.base_url,
            24 * 60 * 60,
        )?
        .url)
}

pub(super) fn playback_url_for_asset(
    session: &LibrarySession,
    path: &Path,
    mime: &str,
) -> AppResult<String> {
    let server = playback_http_server()?;
    let name = path
        .file_name()
        .and_then(|v| v.to_str())
        .unwrap_or("video.mp4");
    let grant = server.downloads.issue_inline(
        path.to_path_buf(),
        name.to_owned(),
        mime.to_owned(),
        &server.base_url,
        24 * 60 * 60,
    )?;
    server
        .downloads
        .bind_library_activity(&grant.download_id, session.asset_activity.clone());
    Ok(grant.url)
}

#[cfg(test)]
#[path = "../../../tests/unit/playback_server.rs"]
mod tests;
