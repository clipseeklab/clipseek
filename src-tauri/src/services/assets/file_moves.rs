//! Reversible, non-overwriting moves for a live asset lifecycle transaction.
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

#[derive(Default)]
pub(super) struct FileMoves(Vec<(PathBuf, PathBuf)>);

impl FileMoves {
    pub(super) fn move_to(&mut self, source: &Path, target: &Path) -> AppResult<()> {
        move_without_replace(source, target)?;
        self.0.push((source.to_path_buf(), target.to_path_buf()));
        Ok(())
    }

    pub(super) fn committed(&mut self) {
        self.0.clear();
    }

    pub(super) fn rollback(&mut self) -> AppResult<()> {
        let mut failures = Vec::new();
        while let Some((source, target)) = self.0.pop() {
            if let Err(error) = move_without_replace(&target, &source) {
                failures.push(format!(
                    "{} -> {}: {error}",
                    target.display(),
                    source.display()
                ));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::coded(
                "asset_rollback_failed",
                failures.join("; "),
            ))
        }
    }
}

impl Drop for FileMoves {
    fn drop(&mut self) {
        // Also compensate during unwinding. Normal failures return these details
        // to the caller; Drop can only log a failed emergency compensation.
        if let Err(error) = self.rollback() {
            tracing::error!(%error, "asset file rollback failed during unwinding");
        }
    }
}

fn remove_created_target(target: &Path, cause: io::Error) -> AppError {
    match fs::remove_file(target) {
        Ok(()) => cause.into(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => cause.into(),
        Err(error) => AppError::coded(
            "asset_file_cleanup_failed",
            format!(
                "{cause}; cannot remove incomplete target {}: {error}",
                target.display()
            ),
        ),
    }
}

pub(super) fn move_without_replace(source: &Path, target: &Path) -> AppResult<()> {
    // A rename may overwrite a collision on Unix. Hard links publish without
    // replacement and preserve metadata on the usual same-volume path.
    if !fs::symlink_metadata(source)?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "asset source is not a regular file",
        )
        .into());
    }
    match fs::hard_link(source, target) {
        Ok(()) => {
            sync_parent(target)?;
            fs::remove_file(source).map_err(|error| remove_created_target(target, error))?;
            return sync_parent(source);
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Err(error.into()),
        Err(_) => {} // Cross-volume or unsupported filesystem: copy exclusively.
    }
    let staging = stage_path(source, target);
    let mut input = fs::File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&staging)?;
    let temporary = tempfile::TempPath::try_from_path(staging)?;
    io::copy(&mut input, &mut output)?;
    output.set_permissions(input.metadata()?.permissions())?;
    output.sync_all()?;
    drop(output);
    drop(input);
    temporary
        .persist_noclobber(target)
        .map_err(|e| AppError::Io(e.error))?;
    sync_parent(target)?;
    fs::remove_file(source).map_err(|error| remove_created_target(target, error))?;
    sync_parent(source)
}

pub(super) fn stage_path(source: &Path, target: &Path) -> PathBuf {
    let digest = blake3::hash(format!("{}\0{}", source.display(), target.display()).as_bytes());
    target.with_file_name(format!(".clipseek-move-{}.partial", digest.to_hex()))
}

pub(super) fn sync_parent(path: &Path) -> AppResult<()> {
    // Unix requires directory fsync for durable link/rename/unlink entries.
    #[cfg(unix)]
    fs::File::open(path.parent().unwrap())?.sync_all()?;
    #[cfg(not(unix))]
    let _ = path;
    Ok(())
}

#[cfg(test)]
#[path = "../../../tests/unit/asset_file_moves.rs"]
mod tests;
