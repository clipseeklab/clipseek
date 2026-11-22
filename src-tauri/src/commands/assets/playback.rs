//! Safe asset/frame previews, playback and platform compatibility repair.

use super::sources::require_source;
use crate::commands::with_session;
use crate::db::{self, model::Asset};
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession};
use crate::AppState;
use serde::Serialize;
use std::{
    fs,
    path::{Path, PathBuf},
};
use tauri::State;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackCompatibilityPreflight {
    pub source_bytes: u64,
    pub estimated_temporary_bytes: u64,
    pub available_bytes: u64,
    pub can_create: bool,
    pub modifies_external_source: bool,
}

#[path = "playback_server.rs"]
mod playback_server;
use playback_server::playback_url_for_asset;

/// Read preview bytes when asset:// loading fails. Resolve paths from the library database;
/// never accept arbitrary client paths.
#[tauri::command]
pub async fn asset_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
) -> AppResult<Vec<u8>> {
    crate::commands::blocking(app, move |state| asset_preview_blocking(state, id)).await
}

pub(crate) fn asset_preview_blocking(state: &AppState, id: i64) -> AppResult<Vec<u8>> {
    with_session(state, |s| {
        let _reading = s.asset_activity.work()?;
        let asset = {
            let conn = s.db.lock().unwrap();
            db::assets::get_asset(&conn, &s.root, id)?.ok_or_else(|| AppError::AssetNotFound)?
        };
        let path = crate::preview::asset_path(&asset.kind, asset.thumb_path, asset.abs_path)?;
        crate::preview::read_image(Path::new(&path))
    })
}

/// Read the matched index unit's preview as an asset:// fallback.
#[tauri::command]
pub async fn frame_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
) -> AppResult<Vec<u8>> {
    crate::commands::blocking(app, move |state| frame_preview_blocking(state, id)).await
}

pub(crate) fn frame_preview_blocking(state: &AppState, id: i64) -> AppResult<Vec<u8>> {
    with_session(state, |s| {
        let _reading = s.asset_activity.work()?;
        let frame = {
            let conn = s.db.lock().unwrap();
            db::assets::get_frame(&conn, &s.root, id)?.ok_or_else(|| {
                AppError::coded("frame_not_found", format!("frame not found: {id}"))
            })?
        };
        let path = frame.abs_path.ok_or_else(|| {
            AppError::coded(
                "frame_no_preview",
                format!("frame has no standalone preview file: {id}"),
            )
        })?;
        crate::preview::read_image(Path::new(&path))
    })
}

/// Return a playback URL, reusing RAW proxies when needed; conversion stays off the main
/// thread.
#[tauri::command]
pub async fn asset_playback_source<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
) -> AppResult<String> {
    crate::commands::blocking(app, move |state| asset_playback_source_blocking(state, id)).await
}

pub(crate) fn asset_playback_source_blocking(state: &AppState, id: i64) -> AppResult<String> {
    // Proxy conversion can take seconds; do not hold the session read lock.
    let session = crate::services::sessions::current(state)?;
    playback_source(&session, id)
}

/// Probe the actual codec after playback failure rather than inferring it from the container
/// extension.
#[tauri::command]
pub async fn asset_playback_codec(
    state: State<'_, AppState>,
    id: i64,
) -> AppResult<Option<String>> {
    let session = crate::services::sessions::current(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        let _reading = session.asset_activity.work()?;
        let asset = video_asset(&session, id)?;
        let source = PathBuf::from(require_source(&session.db.lock().unwrap(), &asset)?);
        Ok(crate::media::ffmpeg::video_codec(&source))
    })
    .await
    .map_err(|error| AppError::Msg(format!("playback codec check failed: {error}")))?
}

fn video_asset(s: &LibrarySession, id: i64) -> AppResult<Asset> {
    let conn = s.db.lock().unwrap();
    let asset =
        db::assets::get_asset(&conn, &s.root, id)?.ok_or_else(|| AppError::AssetNotFound)?;
    if asset.kind != "video" {
        return Err(AppError::coded(
            "asset_type_unsupported",
            "playback proxy is only available for videos",
        ));
    }
    Ok(asset)
}

fn playback_source(s: &LibrarySession, id: i64) -> AppResult<String> {
    let _reading = s.asset_activity.work()?;
    let asset = video_asset(s, id)?;
    let source = PathBuf::from(require_source(&s.db.lock().unwrap(), &asset)?);
    let directory = LibraryDirs::new(&s.root).asset(asset.imported_at, &asset.uid);
    let preview = crate::media::raw_video::preview_source(&source, &directory)?;
    let mime = if preview == source {
        &asset.mime_type
    } else {
        "video/mp4"
    };
    playback_url_for_asset(s, &preview, mime)
}

/// Decode the complete HEIC/HEIF primary image to PNG when the WebView cannot display it.
#[tauri::command]
pub async fn asset_full_preview(app: tauri::AppHandle, id: i64) -> AppResult<tauri::ipc::Response> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |session| {
            let _reading = session.asset_activity.work()?;
            let source = {
                let conn = session.db.lock().unwrap();
                let asset = db::assets::get_asset(&conn, &session.root, id)?
                    .ok_or(AppError::AssetNotFound)?;
                if asset.kind != "image" {
                    return Err(AppError::Msg("full preview requires an image".into()));
                }
                PathBuf::from(require_source(&conn, &asset)?)
            };
            crate::preview::check_size(fs::metadata(&source)?.len())?;
            let bytes = crate::import::thumb::full_heif_preview_png(&source)?;
            crate::preview::check_size(bytes.len() as u64)?;
            Ok(tauri::ipc::Response::new(bytes))
        })
    })
    .await
}
