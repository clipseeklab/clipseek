use super::{activity, check_compatible, write_library_config, LibraryDirs, LIBRARY_CONFIG_FILE};
use crate::config::{GlobalConfig, PendingMigration};
use crate::error::{AppError, AppResult};
use rusqlite::Connection;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub fn migrate_library(config: &mut GlobalConfig, id: &str, new_path: &str) -> AppResult<()> {
    migrate_library_with_progress(config, id, new_path, &|_, _, _| {})
}

pub fn migrate_library_with_progress(
    config: &mut GlobalConfig,
    id: &str,
    new_path: &str,
    on_progress: &dyn Fn(u64, u64, &str),
) -> AppResult<()> {
    migrate_library_with_progress_and_close(config, id, new_path, on_progress, || {})
}

/// Close active handles only after exclusive access succeeds; a busy library stays usable.
pub(crate) fn migrate_library_with_progress_and_close(
    config: &mut GlobalConfig,
    id: &str,
    new_path: &str,
    on_progress: &dyn Fn(u64, u64, &str),
    close_session: impl FnOnce(),
) -> AppResult<()> {
    if let Some(meta) = config.library_by_id(id) {
        check_compatible(Path::new(&meta.path))?;
    }
    let Some(meta_index) = config.libraries.iter().position(|l| l.id == id) else {
        return Err(AppError::LibraryNotFound);
    };
    let old = PathBuf::from(&config.libraries[meta_index].path);
    let activity = activity::for_root(&old)?;
    let migration = activity.cleanup()?;
    close_session();
    let read = crate::db::open_library_read_db(&LibraryDirs::new(&old).db())?;
    if read.query_row(
        "SELECT EXISTS(SELECT 1 FROM meta WHERE key LIKE 'asset_lifecycle.v1.%')",
        [],
        |r| r.get::<_, bool>(0),
    )? {
        return Err(AppError::coded(
            "asset_rollback_failed",
            "Recover interrupted file moves before migrating the library",
        ));
    }
    drop(read);
    let new_root = PathBuf::from(new_path);
    if old == new_root
        || (new_root.exists() && fs::canonicalize(&old).ok() == fs::canonicalize(&new_root).ok())
    {
        return Ok(());
    }
    if new_root.exists() {
        let empty = fs::read_dir(&new_root)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(AppError::coded(
                "library_dir_not_empty",
                "target directory already exists and is not empty",
            ));
        }
        // Remove a verified empty destination first because rename-to-existing-directory
        // behavior varies by platform.
        fs::remove_dir(&new_root)?;
    }
    if let Some(parent) = new_root.parent() {
        fs::create_dir_all(parent)?;
    }
    config.pending_migrations.push(PendingMigration {
        library_id: id.to_string(),
        old_path: old.to_string_lossy().into_owned(),
        new_path: new_root.to_string_lossy().into_owned(),
    });
    config.save()?;
    on_progress(0, 0, "moving_directory");
    if let Err(rename_error) = fs::rename(&old, &new_root) {
        if rename_error.kind() != std::io::ErrorKind::CrossesDevices {
            config.pending_migrations.retain(|p| p.library_id != id);
            config.save()?;
            return Err(rename_error.into());
        }
        // For cross-volume moves, copy into a temporary directory, verify completely, then
        // switch atomically.
        let total = directory_size(&old)?;
        let available = fs2::available_space(new_root.parent().unwrap_or(Path::new("/")))?;
        let reserve = (fs2::total_space(new_root.parent().unwrap_or(Path::new("/")))? / 20)
            .max(1024 * 1024 * 1024);
        let required = total.saturating_add(reserve);
        if available < required {
            return Err(AppError::DiskSpaceInsufficient {
                incoming_bytes: required,
                available_bytes: available,
            });
        }
        // A stable temporary path lets interrupted relocations reuse verified copies.
        let staging = new_root.with_extension(format!("moving-{id}"));
        copy_dir_resumable(&old, &staging, total, on_progress)?;
        validate_library_copy(&staging)?;
        fs::rename(&staging, &new_root)?;
        // Delete the source directory only after the destination is verified and installed.
        fs::remove_dir_all(&old)?;
    }
    migration.retire();
    config.libraries[meta_index].path = new_root.to_string_lossy().into_owned();
    write_library_config(&new_root, &config.libraries[meta_index])?;
    config.pending_migrations.retain(|p| p.library_id != id);
    config.save()?;
    Ok(())
}

pub(super) fn directory_size(path: &Path) -> AppResult<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry =
            entry.map_err(|error| AppError::Msg(format!("library scan failed: {error}")))?;
        let metadata = entry.metadata().map_err(|error| {
            AppError::Msg(format!("migration file metadata read failed: {error}"))
        })?;
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

pub(super) fn copy_dir_resumable(
    src: &Path,
    dst: &Path,
    total: u64,
    on_progress: &dyn Fn(u64, u64, &str),
) -> AppResult<()> {
    fs::create_dir_all(dst)?;
    let mut completed = 0u64;
    for entry in walkdir::WalkDir::new(src).follow_links(false) {
        let entry = entry.map_err(|e| AppError::Msg(format!("migration scan failed: {e}")))?;
        let rel = entry
            .path()
            .strip_prefix(src)
            .map_err(|_| AppError::Msg("invalid migration path".into()))?;
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target)?;
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(AppError::coded(
                "library_symlink_unsupported",
                format!(
                    "library contains a symbolic link and cannot be migrated safely: {}. Remove the link and retry",
                    rel.display()
                ),
            ));
        }
        let size = entry
            .metadata()
            .map_err(|e| AppError::Msg(format!("migration file metadata read failed: {e}")))?
            .len();
        // Compare content when resuming; equal file lengths do not prove an interrupted copy
        // is complete.
        let target_complete = target.metadata().map(|m| m.len() == size).unwrap_or(false)
            && crate::import::scanner::file_hash(entry.path()).ok()
                == crate::import::scanner::file_hash(&target).ok();
        if target_complete {
            completed += size;
            on_progress(completed, total, rel.to_string_lossy().as_ref());
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let part = target.with_extension("part");
        fs::copy(entry.path(), &part)?;
        if part.metadata()?.len() != size {
            return Err(AppError::coded(
                "file_copy_incomplete",
                format!("incomplete file copy: {}", rel.display()),
            ));
        }
        fs::rename(part, &target)?;
        completed += size;
        on_progress(completed, total, rel.to_string_lossy().as_ref());
    }
    Ok(())
}

pub(super) fn validate_library_copy(root: &Path) -> AppResult<()> {
    if !root.join(LIBRARY_CONFIG_FILE).is_file()
        || !root.join(".clipstudio-open/library.db").is_file()
    {
        return Err(AppError::coded(
            "library_copy_incomplete",
            "migrated library is missing its config or database file",
        ));
    }
    let conn = Connection::open(root.join(".clipstudio-open/library.db"))?;
    let result: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if result != "ok" {
        return Err(AppError::coded(
            "library_db_check_failed",
            format!("migrated database verification failed: {result}"),
        ));
    }
    Ok(())
}
