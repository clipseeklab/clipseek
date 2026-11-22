//! Collision-safe file export and video/subtitle clip export.

use super::sources::require_source;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::AppState;
use std::{
    fs,
    path::{Path, PathBuf},
};

/// Export to a user-selected directory, appending a sequence number for duplicate names.
#[tauri::command]
pub async fn asset_export<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
    target_dir: String,
) -> AppResult<Vec<String>> {
    let result = crate::commands::blocking(app, move |state| {
        asset_export_blocking(state, ids, target_dir)
    })
    .await;

    result
}

pub(crate) fn asset_export_blocking(
    state: &AppState,
    ids: Vec<i64>,
    target_dir: String,
) -> AppResult<Vec<String>> {
    let session = crate::services::sessions::current(state)?;
    let s = session.as_ref();
    let _reading = s.asset_activity.work()?;
    let dir = PathBuf::from(target_dir);
    if !dir.is_dir() {
        return Err(AppError::coded(
            "export_target_invalid",
            "export target is not a valid folder",
        ));
    }
    let mut exported = Vec::new();
    for id in ids {
        let (asset, source) = {
            let conn = s.db.lock().unwrap();
            let asset = db::assets::get_asset(&conn, &s.root, id)?
                .ok_or_else(|| AppError::AssetNotFound)?;
            let source = require_source(&conn, &asset)?;
            (asset, source)
        };
        let dest = copy_export_file(&dir, &asset.file_name, Path::new(&source))?;
        exported.push(dest.to_string_lossy().into_owned());
    }
    Ok(exported)
}

/// Atomically reserve each export name, including when an existing entry is a
/// dangling symlink. Copy through the handle so the destination is never reopened.
fn copy_export_file(dir: &Path, file_name: &str, source: &Path) -> AppResult<PathBuf> {
    if file_name.contains(['/', '\\'])
        || Path::new(file_name)
            .file_name()
            .and_then(|name| name.to_str())
            != Some(file_name)
    {
        return Err(AppError::InvalidArgument(
            "export filename must be a single filename".into(),
        ));
    }
    let mut input = fs::File::open(source)?;
    let metadata = input.metadata()?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "export source is not a file",
        )
        .into());
    }
    let (stem, ext) = file_name.rsplit_once('.').unwrap_or((file_name, ""));
    for n in 0u64.. {
        let name = if n == 0 {
            file_name.to_owned()
        } else if ext.is_empty() {
            format!("{stem} ({n})")
        } else {
            format!("{stem} ({n}).{ext}")
        };
        let dest = dir.join(name);
        let mut output = match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            // Windows reports AccessDenied for a directory occupying this name.
            // Only retry confirmed collisions; preserve genuine permission errors.
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied
                    && fs::symlink_metadata(&dest).is_ok() =>
            {
                continue
            }
            Err(error) => return Err(error.into()),
        };
        let result = std::io::copy(&mut input, &mut output)
            .and_then(|_| output.set_permissions(metadata.permissions()));
        if let Err(error) = result {
            drop(output);
            let _ = fs::remove_file(&dest);
            return Err(error.into());
        }
        return Ok(dest);
    }
    unreachable!()
}
