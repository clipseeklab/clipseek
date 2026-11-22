use crate::{
    db,
    error::{AppError, AppResult},
    media::metadata::{self, MediaMetadata},
};

#[tauri::command]
pub async fn asset_metadata(
    app: tauri::AppHandle,
    id: i64,
    context: crate::commands::body::LibraryContext,
) -> AppResult<MediaMetadata> {
    crate::commands::blocking_in_library(app, context, move |_, session| {
        let _work = session.asset_activity.work()?;
        let asset = db::assets::get_asset(&session.read_db(), &session.root, id)?
            .ok_or(AppError::AssetNotFound)?;
        metadata::read(std::path::Path::new(&asset.abs_path))
    })
    .await
}
