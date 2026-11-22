//! Application services and shared workflows, independent of command handlers.

pub mod assets;
pub mod folders;
pub mod imports;
pub mod maintenance;
pub mod search;
pub mod sessions;

use std::sync::Arc;

use rusqlite::params;

use crate::db::{
    self,
    model::{Asset, AssetDetail, AssetFilter, Folder, Paged, Tag},
};
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::{vector, AppState};

pub fn session(state: &AppState, library_id: Option<&str>) -> AppResult<Arc<LibrarySession>> {
    sessions::resolve(state, library_id)
}

pub fn list_assets(session: &LibrarySession, filter: &AssetFilter) -> AppResult<Paged<Asset>> {
    let conn = session.read_db();
    db::assets::query_assets(&conn, &session.root, filter)
}

pub fn asset_detail(session: &LibrarySession, uid: &str) -> AppResult<AssetDetail> {
    let conn = session.read_db();
    db::assets::get_asset_detail_by_uid(&conn, &session.root, uid)?
        .filter(|detail| detail.asset.deleted_at.is_none())
        .ok_or(AppError::AssetNotFound)
}

pub fn folders_and_tags(session: &LibrarySession) -> AppResult<(Vec<Folder>, Vec<Tag>)> {
    let conn = session.read_db();
    Ok((db::folders::list(&conn)?, db::tags::list(&conn)?))
}

pub fn create_folder(session: &LibrarySession, name: &str) -> AppResult<Folder> {
    let name = folders::validate_folder_name(name)?;
    folders::create_folder(session, &name)
}

pub fn update_asset_tags(
    session: &LibrarySession,
    uid: &str,
    add: &[String],
    remove: &[String],
) -> AppResult<Vec<String>> {
    if add
        .iter()
        .chain(remove)
        .any(|name| name.trim().is_empty() || name.contains(','))
    {
        return Err(AppError::InvalidArgument(
            "tag names cannot be empty or contain commas".into(),
        ));
    }
    let asset_id = {
        let mut conn = session.db.lock().unwrap();
        let asset = db::assets::get_asset_by_uid(&conn, &session.root, uid)?
            .filter(|asset| asset.deleted_at.is_none())
            .ok_or(AppError::AssetNotFound)?;
        let tx = conn.transaction()?;
        db::tags::add_to_asset(&tx, asset.id, add)?;
        for raw in remove {
            let name = raw.trim();
            if name.is_empty() {
                continue;
            }
            tx.execute(
                "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id IN \
                 (SELECT id FROM tags WHERE name = ?2)",
                params![asset.id, name],
            )?;
        }
        tx.commit()?;
        asset.id
    };
    vector::sync_asset_payload(session, asset_id)?;
    let conn = session.read_db();
    Ok(db::assets::get_asset(&conn, &session.root, asset_id)?
        .map(|asset| asset.tags)
        .unwrap_or_default())
}

pub fn move_assets_to_folder(
    session: &LibrarySession,
    uids: &[String],
    folder_id: Option<i64>,
) -> AppResult<usize> {
    if uids.is_empty() || uids.len() > 100 {
        return Err(AppError::InvalidArgument(
            "assetUids must contain 1..100 items".into(),
        ));
    }
    let ids = {
        let conn = session.read_db();
        let mut ids = Vec::with_capacity(uids.len());
        for uid in uids {
            let asset = db::assets::get_asset_by_uid(&conn, &session.root, uid)?
                .filter(|asset| asset.deleted_at.is_none())
                .ok_or(AppError::AssetNotFound)?;
            ids.push(asset.id);
        }
        ids
    };
    assets::move_assets_to_folder(session, &ids, folder_id)?;
    Ok(ids.len())
}

pub fn semantic_search(
    state: &AppState,
    session: &LibrarySession,
    query: &str,
    filter: &AssetFilter,
    page: u32,
    page_size: u32,
) -> AppResult<Paged<search::SearchHit>> {
    let query = query.trim();
    if query.is_empty() {
        return Err(AppError::InvalidArgument("semanticQuery is empty".into()));
    }
    let vector = state.clip.encode_text_cached(query)?;
    search::run_search_session(session, vector, Some(filter), Some(page), Some(page_size))
}
