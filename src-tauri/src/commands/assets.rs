use fs2::{available_space, total_space};
use rusqlite::params;
use serde::Serialize;

use crate::commands::with_session;
use crate::db::{
    self,
    model::{Asset, AssetDetail, AssetFilter, Paged},
};
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::util;
use crate::vector;
use crate::AppState;

use crate::services::assets::{
    delete_permanent, move_assets_to_folder, restore_assets, trash_assets,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStorageStats {
    pub asset_count: u64,
    pub library_bytes: u64,
    pub disk_available_bytes: u64,
    pub disk_total_bytes: u64,
}

/// Storage statistics for the current library and its volume.
#[tauri::command]
pub async fn asset_storage_stats(app: tauri::AppHandle) -> AppResult<LibraryStorageStats> {
    crate::commands::blocking(app, asset_storage_stats_blocking).await
}

fn asset_storage_stats_blocking(state: &AppState) -> AppResult<LibraryStorageStats> {
    with_session(state, |s| {
        let asset_count = {
            let conn = s.read_db();
            conn.query_row(
                "SELECT COUNT(*) FROM assets WHERE deleted_at IS NULL",
                [],
                |row| row.get::<_, u64>(0),
            )?
        };
        let library_root = &s.root;
        Ok(LibraryStorageStats {
            asset_count,
            library_bytes: directory_size(library_root),
            disk_available_bytes: available_space(library_root)?,
            disk_total_bytes: total_space(library_root)?,
        })
    })
}

fn directory_size(path: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

/// Paginated asset listing shared by asset, folder, uncategorized, and trash pages.
#[tauri::command]
pub async fn asset_list<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    filter: AssetFilter,
    context: super::body::LibraryContext,
) -> AppResult<Paged<Asset>> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        crate::services::list_assets(session, &filter)
    })
    .await
}

/// Asset details, including internal index units.
#[tauri::command]
pub async fn asset_detail(
    app: tauri::AppHandle,
    id: i64,
    context: super::body::LibraryContext,
) -> AppResult<AssetDetail> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        asset_detail_blocking(session, id)
    })
    .await
}

#[tauri::command]
pub async fn asset_set_cover<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
    frame_id: i64,
    context: super::body::LibraryContext,
) -> AppResult<Asset> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        let _work = session.asset_activity.work()?;
        let conn = session.db.lock().unwrap();
        db::assets::set_video_cover(&conn, &session.root, id, frame_id)
    })
    .await
}

#[tauri::command]
pub async fn asset_set_display_name<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
    display_name: Option<String>,
    context: super::body::LibraryContext,
) -> AppResult<()> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        asset_set_display_name_blocking(session, id, display_name)
    })
    .await
}

pub(crate) fn asset_set_display_name_blocking(
    session: &LibrarySession,
    id: i64,
    display_name: Option<String>,
) -> AppResult<()> {
    let display_name = display_name
        .map(|value| value.trim().chars().take(500).collect::<String>())
        .filter(|value| !value.is_empty());

    let conn = session.db.lock().unwrap();
    if db::assets::get_asset(&conn, &session.root, id)?.is_none() {
        return Err(AppError::AssetNotFound);
    }
    db::assets::set_display_name(&conn, id, display_name.as_deref())
}

fn asset_detail_blocking(s: &LibrarySession, id: i64) -> AppResult<AssetDetail> {
    let _reading = s.asset_activity.work()?;
    let asset = {
        let conn = s.db.lock().unwrap();
        db::assets::get_asset(&conn, &s.root, id)?.ok_or_else(|| AppError::AssetNotFound)?
    };
    let invalid_dimension = |dimension: Option<i64>| !matches!(dimension, Some(value) if (1..=100_000).contains(&value));
    // Mark missing reference sources before later operations repeatedly fail on I/O.
    if asset.storage == "reference" {
        let missing = !std::path::Path::new(&asset.abs_path).is_file();
        if missing != asset.source_missing {
            let conn = s.db.lock().unwrap();
            let _ = db::assets::set_source_missing(&conn, id, missing);
        }
    }
    // Repair legacy dimensions that accidentally stored codec tags such as 0x31307661.
    if asset.kind == "video" && (invalid_dimension(asset.width) || invalid_dimension(asset.height))
    {
        if let Ok(info) = crate::media::ffmpeg::probe(std::path::Path::new(&asset.abs_path)) {
            let conn = s.db.lock().unwrap();
            db::assets::update_asset_probe(
                &conn,
                id,
                crate::db::assets::AssetProbe {
                    width: info.width,
                    height: info.height,
                    duration_ms: info.duration_ms,
                    color: None,
                    thumb_rel: None,
                },
            )?;
        }
    }
    let conn = s.db.lock().unwrap();
    db::assets::get_asset_detail(&conn, &s.root, id)?.ok_or_else(|| AppError::AssetNotFound)
}

/// Add asset tags and synchronize vector payloads.
#[tauri::command]
pub async fn asset_add_tags<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
    names: Vec<String>,
    context: super::body::LibraryContext,
) -> AppResult<Vec<String>> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        asset_add_tags_blocking(session, id, names)
    })
    .await
}

pub(crate) fn asset_add_tags_blocking(
    s: &LibrarySession,
    id: i64,
    names: Vec<String>,
) -> AppResult<Vec<String>> {
    {
        let conn = s.db.lock().unwrap();
        db::tags::add_to_asset(&conn, id, &names)?;
    }
    vector::sync_asset_payload(s, id)?;
    let conn = s.db.lock().unwrap();
    Ok(db::assets::get_asset(&conn, &s.root, id)?
        .map(|a| a.tags)
        .unwrap_or_default())
}

/// Remove one tag by name; detail responses expose names only.
#[tauri::command]
pub async fn asset_remove_tag<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
    tag_name: String,
    context: super::body::LibraryContext,
) -> AppResult<Vec<String>> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        asset_remove_tag_blocking(session, id, tag_name)
    })
    .await
}

pub(crate) fn asset_remove_tag_blocking(
    s: &LibrarySession,
    id: i64,
    tag_name: String,
) -> AppResult<Vec<String>> {
    {
        let conn = s.db.lock().unwrap();
        let tag_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1",
                params![tag_name],
                |r| r.get(0),
            )
            .unwrap_or(None);
        if let Some(tag_id) = tag_id {
            db::tags::remove_from_asset(&conn, id, tag_id)?;
        }
    }
    vector::sync_asset_payload(s, id)?;
    let conn = s.db.lock().unwrap();
    Ok(db::assets::get_asset(&conn, &s.root, id)?
        .map(|a| a.tags)
        .unwrap_or_default())
}

/// Change logical folder membership without moving physical files.
#[tauri::command]
pub async fn asset_move_to_folder<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
    folder_id: Option<i64>,
    context: super::body::LibraryContext,
) -> AppResult<()> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        move_assets_to_folder(session, &ids, folder_id)
    })
    .await
}

/// Move assets to trash.
#[tauri::command]
pub async fn asset_trash<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
    context: super::body::LibraryContext,
) -> AppResult<()> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        trash_assets(session, &ids)
    })
    .await
}

/// Restore assets from trash.
#[tauri::command]
pub async fn asset_restore<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| asset_restore_blocking(state, ids)).await
}

pub(crate) fn asset_restore_blocking(state: &AppState, ids: Vec<i64>) -> AppResult<()> {
    with_session(state, |s| {
        let _incoming = {
            let conn = s.db.lock().unwrap();
            let mut total = 0u64;
            for id in ids
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
            {
                if let Some(asset) = db::assets::get_asset(&conn, &s.root, id)? {
                    if asset.deleted_at.is_some() {
                        total = total.saturating_add(asset.file_size.max(0) as u64);
                    }
                }
            }
            total
        };

        restore_assets(s, &ids)
    })
}

/// Permanently remove records, derived files, and vectors; preserve referenced source files.
#[tauri::command]
pub async fn asset_delete_permanent<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
    context: super::body::LibraryContext,
) -> AppResult<()> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        delete_permanent(session, &ids)
    })
    .await
}

/// Manually empty the trash.
#[tauri::command]
pub async fn trash_purge_all<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppResult<()> {
    crate::commands::blocking(app, trash_purge_all_blocking).await
}

pub(crate) fn trash_purge_all_blocking(state: &AppState) -> AppResult<()> {
    let ids: Vec<i64> = with_session(state, |s| {
        let conn = s.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT id FROM assets WHERE deleted_at IS NOT NULL")?;
        let ids = stmt
            .query_map([], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    })?;
    with_session(state, |s| delete_permanent(s, &ids))
}

/// Clean expired trash on startup and daily according to trash_retention_days.
pub fn auto_purge_expired(s: &LibrarySession, retention_days: u32) -> AppResult<usize> {
    let cutoff = util::now() - (retention_days as i64) * 86_400;
    let ids: Vec<i64> = {
        let conn = s.db.lock().unwrap();
        db::asset_cleanup::expired_or_pending(&conn, cutoff)?
    };
    if !ids.is_empty() {
        delete_permanent(s, &ids)?;
    }
    Ok(ids.len())
}

/// Guard source access: mark missing references and return source_file_missing. Legacy
/// managed assets resolve to their library copy.
mod export;
mod metadata;
mod playback;
mod sources;

pub use export::*;
pub use metadata::*;
pub use playback::*;
pub use sources::*;
