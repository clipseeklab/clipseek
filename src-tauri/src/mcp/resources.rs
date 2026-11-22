//! Registered preview resources with library and path validation.

use super::validate_uid;
use crate::error::{AppError, AppResult};
use crate::{services, AppState};
use std::{fs, path::PathBuf};

pub(super) fn read_preview(state: &AppState, uri: &str) -> AppResult<(Vec<u8>, String)> {
    let rest = uri
        .strip_prefix("clipstudio-open://libraries/")
        .ok_or_else(|| AppError::InvalidArgument("invalid resource URI".into()))?;
    let parts = rest.split('/').collect::<Vec<_>>();
    if !(parts.len() == 3 || parts.len() == 4) {
        return Err(AppError::InvalidArgument("invalid resource URI".into()));
    }
    let library_id = parts[0];
    let session = services::session(state, Some(library_id))?;
    let _reading = session.asset_activity.work()?;
    let path = match parts.as_slice() {
        [_, "assets", asset_uid, "preview"] => {
            validate_uid(asset_uid)?;
            let detail = services::asset_detail(&session, asset_uid)?;
            detail.asset.thumb_path.ok_or(AppError::AssetNotFound)?
        }
        _ => return Err(AppError::InvalidArgument("invalid resource URI".into())),
    };
    let path = PathBuf::from(path);
    let canonical = fs::canonicalize(&path)?;
    let root = fs::canonicalize(&session.root)?;
    if !canonical.starts_with(root.join("images")) {
        return Err(AppError::Unauthorized);
    }
    let mime = match canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        _ => return Err(AppError::InvalidArgument("unsupported preview type".into())),
    };
    let metadata = fs::metadata(&canonical)?;
    if metadata.len() > 20 * 1024 * 1024 {
        return Err(AppError::InvalidArgument("preview exceeds 20 MiB".into()));
    }
    Ok((fs::read(canonical)?, mime.into()))
}
