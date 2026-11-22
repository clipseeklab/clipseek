//! Capability-based streaming downloads used by the MCP creation interface.

struct GrantOptions {
    delete_on_expire: bool,
    expected_epoch: u64,
    ttl_seconds: i64,
    inline: bool,
}

use std::collections::HashMap;
use std::path::{Path as FsPath, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::http::{header, HeaderValue, Method, Response, StatusCode};
use percent_encoding::{utf8_percent_encode, NON_ALPHANUMERIC};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

use crate::error::{AppError, AppResult};

pub const GRANT_TTL_SECONDS: i64 = 10 * 60;
const DOWNLOADS_PER_MINUTE: u32 = 120;
const DERIVATIVE_GRANT_MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub download_id: String,
    pub url: String,
    pub file_name: String,
    pub mime_type: String,
    pub size: u64,
    pub expires_at: i64,
}

#[derive(Clone)]
struct DownloadGrant {
    token_hash: [u8; 32],
    path: PathBuf,
    file_name: String,
    mime_type: String,
    size: u64,
    expires_at: i64,
    delete_on_expire: bool,
    inline: bool,
    activity: Option<Arc<crate::library::activity::AssetActivity>>,
}

pub struct DownloadRegistry {
    grants: Mutex<HashMap<String, DownloadGrant>>,
    rate: Mutex<HashMap<String, (i64, u32)>>,
    epoch: AtomicU64,
    concurrency: Arc<tokio::sync::Semaphore>,
    local_playback: bool,
}

impl Default for DownloadRegistry {
    fn default() -> Self {
        Self {
            grants: Mutex::new(HashMap::new()),
            rate: Mutex::new(HashMap::new()),
            epoch: AtomicU64::new(0),
            concurrency: Arc::new(tokio::sync::Semaphore::new(4)),
            local_playback: false,
        }
    }
}

struct PermittedStream {
    inner:
        Pin<Box<dyn futures_core::Stream<Item = Result<axum::body::Bytes, std::io::Error>> + Send>>,
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
    _activity: Option<crate::library::activity::OwnedActivityGuard>,
}

impl futures_core::Stream for PermittedStream {
    type Item = Result<axum::body::Bytes, std::io::Error>;

    fn poll_next(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(context)
    }
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    token: String,
}

impl DownloadRegistry {
    /// Only for the loopback-only client playback server. A video seek can issue
    /// hundreds of byte-range requests; MCP download quotas cannot count them
    /// as separate downloads. Keep token checks and stream file data on demand.
    pub fn for_local_playback() -> Self {
        Self {
            local_playback: true,
            ..Self::default()
        }
    }

    pub fn issue(
        &self,
        path: PathBuf,
        file_name: String,
        mime_type: String,
        base_url: &str,
        delete_on_expire: bool,
    ) -> AppResult<DownloadInfo> {
        let epoch = self.epoch.load(Ordering::SeqCst);
        self.issue_at_epoch_with_options(
            path,
            file_name,
            mime_type,
            base_url,
            GrantOptions {
                delete_on_expire,
                expected_epoch: epoch,
                ttl_seconds: GRANT_TTL_SECONDS,
                inline: false,
            },
        )
    }

    /// Issue a long-lived, inline capability for local media playback.
    pub fn issue_inline(
        &self,
        path: PathBuf,
        file_name: String,
        mime_type: String,
        base_url: &str,
        ttl_seconds: i64,
    ) -> AppResult<DownloadInfo> {
        let epoch = self.epoch.load(Ordering::SeqCst);
        self.issue_at_epoch_with_options(
            path,
            file_name,
            mime_type,
            base_url,
            GrantOptions {
                delete_on_expire: false,
                expected_epoch: epoch,
                ttl_seconds,
                inline: true,
            },
        )
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }

    pub fn issue_at_epoch(
        &self,
        path: PathBuf,
        file_name: String,
        mime_type: String,
        base_url: &str,
        delete_on_expire: bool,
        expected_epoch: u64,
    ) -> AppResult<DownloadInfo> {
        self.issue_at_epoch_with_options(
            path,
            file_name,
            mime_type,
            base_url,
            GrantOptions {
                delete_on_expire,
                expected_epoch,
                ttl_seconds: GRANT_TTL_SECONDS,
                inline: false,
            },
        )
    }

    fn issue_at_epoch_with_options(
        &self,
        path: PathBuf,
        file_name: String,
        mime_type: String,
        base_url: &str,
        options: GrantOptions,
    ) -> AppResult<DownloadInfo> {
        let GrantOptions {
            delete_on_expire,
            expected_epoch,
            ttl_seconds,
            inline,
        } = options;
        if self.epoch.load(Ordering::SeqCst) != expected_epoch {
            return Err(AppError::Unauthorized);
        }
        self.cleanup();
        let metadata = std::fs::metadata(&path)?;
        if !metadata.is_file() {
            return Err(AppError::DownloadNotFound);
        }
        let mut token_bytes = [0_u8; 32];
        rand::rng().fill_bytes(&mut token_bytes);
        let token = hex::encode(token_bytes);
        let token_hash: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        let download_id = crate::util::new_id();
        let expires_at = crate::util::now() + ttl_seconds.max(1);
        let encoded_name = utf8_percent_encode(&file_name, NON_ALPHANUMERIC).to_string();
        let url = format!(
            "{}/files/{download_id}/{encoded_name}?token={token}",
            base_url.trim_end_matches('/')
        );
        let mut grants = self.grants.lock().unwrap();
        grants.insert(
            download_id.clone(),
            DownloadGrant {
                token_hash,
                path,
                file_name: file_name.clone(),
                mime_type: mime_type.clone(),
                size: metadata.len(),
                expires_at,
                delete_on_expire,
                inline,
                activity: None,
            },
        );
        if delete_on_expire {
            let mut derived = grants
                .iter()
                .filter(|(_, grant)| grant.delete_on_expire)
                .map(|(id, grant)| (id.clone(), grant.expires_at, grant.size))
                .collect::<Vec<_>>();
            derived.sort_by_key(|(_, expires_at, _)| *expires_at);
            let mut total = derived.iter().map(|(_, _, size)| size).sum::<u64>();
            for (id, _, size) in derived {
                if total <= DERIVATIVE_GRANT_MAX_BYTES {
                    break;
                }
                if let Some(old) = grants.remove(&id) {
                    let _ = std::fs::remove_file(old.path);
                    total = total.saturating_sub(size);
                }
            }
            if !grants.contains_key(&download_id) {
                return Err(AppError::DerivativeFailed(
                    "derivative cache limit exceeded".into(),
                ));
            }
        }
        if self.epoch.load(Ordering::SeqCst) != expected_epoch {
            if let Some(revoked) = grants.remove(&download_id) {
                if revoked.delete_on_expire {
                    let _ = std::fs::remove_file(revoked.path);
                }
            }
            return Err(AppError::Unauthorized);
        }
        drop(grants);
        Ok(DownloadInfo {
            download_id,
            url,
            file_name,
            mime_type,
            size: metadata.len(),
            expires_at,
        })
    }

    pub(crate) fn bind_library_activity(
        &self,
        id: &str,
        activity: Arc<crate::library::activity::AssetActivity>,
    ) {
        if let Some(grant) = self.grants.lock().unwrap().get_mut(id) {
            grant.activity = Some(activity);
        }
    }

    pub fn revoke_all(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let grants = std::mem::take(&mut *self.grants.lock().unwrap());
        for grant in grants.into_values().filter(|grant| grant.delete_on_expire) {
            let _ = std::fs::remove_file(grant.path);
        }
    }

    fn cleanup(&self) {
        let now = crate::util::now();
        let mut grants = self.grants.lock().unwrap();
        let expired = grants
            .iter()
            .filter(|(_, grant)| grant.expires_at <= now)
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        for id in expired {
            if let Some(grant) = grants.remove(&id) {
                if grant.delete_on_expire {
                    let _ = std::fs::remove_file(grant.path);
                }
            }
        }
    }

    fn get(&self, id: &str, token: &str) -> Result<DownloadGrant, AppError> {
        let mut grants = self.grants.lock().unwrap();
        let grant = grants.get(id).cloned().ok_or(AppError::DownloadNotFound)?;
        if grant.expires_at <= crate::util::now() {
            grants.remove(id);
            if grant.delete_on_expire {
                let _ = std::fs::remove_file(&grant.path);
            }
            return Err(AppError::DownloadExpired);
        }
        drop(grants);
        let supplied: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        if !constant_time_equal(&grant.token_hash, &supplied) {
            return Err(AppError::DownloadNotFound);
        }
        Ok(grant)
    }

    fn allow(&self, remote: &str) -> bool {
        let minute = crate::util::now() / 60;
        let mut rate = self.rate.lock().unwrap();
        let entry = rate.entry(remote.to_string()).or_insert((minute, 0));
        if entry.0 != minute {
            *entry = (minute, 0);
        }
        if entry.1 >= DOWNLOADS_PER_MINUTE {
            return false;
        }
        entry.1 += 1;
        true
    }
}

fn constant_time_equal(a: &[u8; 32], b: &[u8; 32]) -> bool {
    a.iter()
        .zip(b)
        .fold(0_u8, |diff, (left, right)| diff | (left ^ right))
        == 0
}

fn error_response(status: StatusCode, code: &str) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(format!(r#"{{"code":"{code}"}}"#)))
        .unwrap()
}

pub async fn download(
    State(registry): State<Arc<DownloadRegistry>>,
    Path((download_id, _requested_name)): Path<(String, String)>,
    Query(query): Query<DownloadQuery>,
    request: Request,
) -> Response<Body> {
    let remote = request
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|value| value.0.ip().to_string())
        .unwrap_or_else(|| "unknown".into());
    if !registry.local_playback && !registry.allow(&remote) {
        return error_response(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }
    let grant = match registry.get(&download_id, &query.token) {
        Ok(grant) => grant,
        Err(AppError::DownloadExpired) => {
            return error_response(StatusCode::GONE, "download_expired")
        }
        Err(_) => return error_response(StatusCode::NOT_FOUND, "download_not_found"),
    };
    let activity = match grant.activity.as_ref() {
        Some(activity) => activity.owned_work().map(Some),
        None => crate::library::activity::work_for_path(&grant.path),
    };
    let activity = match activity {
        Ok(guard) => guard,
        Err(_) => return error_response(StatusCode::CONFLICT, "operation_busy"),
    };
    let permit = if registry.local_playback {
        None
    } else {
        match registry.concurrency.clone().try_acquire_owned() {
            Ok(permit) => Some(permit),
            Err(_) => return error_response(StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
        }
    };
    if grant.size == 0 {
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, &grant.mime_type)
            .header(header::CONTENT_LENGTH, 0)
            .header(header::CACHE_CONTROL, "private, no-store")
            .header("x-content-type-options", "nosniff")
            .body(Body::empty())
            .unwrap();
    }
    let (start, end, partial) = match parse_range(request.headers().get(header::RANGE), grant.size)
    {
        Ok(range) => range,
        Err(()) => {
            let mut response = error_response(StatusCode::RANGE_NOT_SATISFIABLE, "invalid_range");
            response.headers_mut().insert(
                header::CONTENT_RANGE,
                HeaderValue::from_str(&format!("bytes */{}", grant.size)).unwrap(),
            );
            return response;
        }
    };
    let length = end.saturating_sub(start) + 1;
    let disposition = format!(
        "{}; filename*=UTF-8''{}",
        if grant.inline { "inline" } else { "attachment" },
        utf8_percent_encode(&grant.file_name, NON_ALPHANUMERIC)
    );
    let mut builder = Response::builder()
        .status(if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        })
        .header(header::CONTENT_TYPE, &grant.mime_type)
        .header(header::CONTENT_LENGTH, length)
        .header(header::ACCEPT_RANGES, "bytes")
        .header(header::CONTENT_DISPOSITION, disposition)
        .header(header::CACHE_CONTROL, "private, no-store")
        .header("x-content-type-options", "nosniff");
    if partial {
        builder = builder.header(
            header::CONTENT_RANGE,
            format!("bytes {start}-{end}/{}", grant.size),
        );
    }
    if request.method() == Method::HEAD {
        return builder.body(Body::empty()).unwrap();
    }
    let mut file = match tokio::fs::File::open(&grant.path).await {
        Ok(file) => file,
        Err(_) => return error_response(StatusCode::NOT_FOUND, "download_not_found"),
    };
    if file.seek(std::io::SeekFrom::Start(start)).await.is_err() {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, "io");
    }
    let stream = PermittedStream {
        inner: Box::pin(ReaderStream::new(file.take(length))),
        _permit: permit,
        _activity: activity,
    };
    builder.body(Body::from_stream(stream)).unwrap()
}

fn parse_range(value: Option<&HeaderValue>, size: u64) -> Result<(u64, u64, bool), ()> {
    if size == 0 {
        return Ok((0, 0, false));
    }
    let Some(value) = value else {
        return Ok((0, size - 1, false));
    };
    let raw = value.to_str().map_err(|_| ())?;
    let raw = raw.strip_prefix("bytes=").ok_or(())?;
    if raw.contains(',') {
        return Err(());
    }
    let (start, end) = raw.split_once('-').ok_or(())?;
    let (start, end) = if start.is_empty() {
        let suffix = end.parse::<u64>().map_err(|_| ())?;
        if suffix == 0 {
            return Err(());
        }
        (size.saturating_sub(suffix), size - 1)
    } else {
        let start = start.parse::<u64>().map_err(|_| ())?;
        let end = if end.is_empty() {
            size - 1
        } else {
            end.parse::<u64>().map_err(|_| ())?.min(size - 1)
        };
        (start, end)
    };
    if start >= size || start > end {
        return Err(());
    }
    Ok((start, end, true))
}

pub fn canonical_library_asset(root: &FsPath, path: &FsPath) -> AppResult<PathBuf> {
    let canonical = std::fs::canonicalize(path)?;
    let image_root = std::fs::canonicalize(root)?.join("images");
    if !canonical.starts_with(image_root) {
        return Err(AppError::Unauthorized);
    }
    Ok(canonical)
}

/// Validate registered source access. Legacy managed assets must remain under images;
/// references permit only the exact database source, never arbitrary client paths.
pub fn canonical_asset_source(
    root: &FsPath,
    asset: &crate::db::model::Asset,
) -> AppResult<PathBuf> {
    let path = std::path::PathBuf::from(&asset.abs_path);
    if asset.storage == "reference" {
        // Canonicalization also checks existence; missing references are rejected explicitly.
        return std::fs::canonicalize(&path).map_err(|_| AppError::Unauthorized);
    }
    canonical_library_asset(root, &path)
}

#[cfg(test)]
#[path = "../tests/unit/mcp_media.rs"]
mod tests;
