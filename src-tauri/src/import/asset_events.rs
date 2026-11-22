//! Capture a consistent database snapshot, then notify observers without holding its mutex.
use crate::db;
use crate::db::model::Asset;
use crate::error::AppResult;
use crate::library::LibrarySession;

pub(super) fn set_stage(
    session: &LibrarySession,
    asset_id: i64,
    stage: &str,
    on_asset: &dyn Fn(&Asset),
) -> AppResult<()> {
    session.ensure_asset_active(asset_id)?;
    let asset = {
        let conn = session.db.lock().unwrap();
        db::assets::set_import_stage(&conn, asset_id, stage)?;
        db::assets::get_asset(&conn, &session.root, asset_id)?
    };
    if let Some(asset) = asset {
        on_asset(&asset);
    }
    session.ensure_asset_active(asset_id)
}

pub(super) fn on_asset_and_fetch(
    session: &LibrarySession,
    asset_id: i64,
    on_asset: &dyn Fn(&Asset),
) -> AppResult<()> {
    let asset = {
        let conn = session.db.lock().unwrap();
        db::assets::get_asset(&conn, &session.root, asset_id)?
    };
    if let Some(asset) = asset {
        on_asset(&asset);
    }
    Ok(())
}
