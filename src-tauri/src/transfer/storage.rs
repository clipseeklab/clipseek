//! Capacity checks for cross-library transfers.
use crate::error::{AppError, AppResult};
use fs2::{available_space, total_space};
use std::path::Path;

pub(super) fn directory_size(path: &Path) -> AppResult<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry =
            entry.map_err(|error| AppError::Msg(format!("asset file scan failed: {error}")))?;
        let metadata = entry
            .metadata()
            .map_err(|error| AppError::Msg(format!("asset file metadata read failed: {error}")))?;
        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }
    }
    Ok(total)
}

/// Reserve the larger of 1 GiB or 5% of capacity so WAL growth and index optimization cannot
/// exhaust disk.
pub(super) fn ensure_transfer_space(target: &Path, copy_bytes: u64) -> AppResult<()> {
    let available = available_space(target)?;
    let required = required_transfer_space(copy_bytes, total_space(target)?);
    if available < required {
        return Err(AppError::DiskSpaceInsufficient {
            incoming_bytes: required,
            available_bytes: available,
        });
    }
    Ok(())
}

fn required_transfer_space(copy_bytes: u64, disk_total: u64) -> u64 {
    copy_bytes.saturating_add((disk_total / 20).max(1024 * 1024 * 1024))
}

#[cfg(test)]
#[path = "../../tests/unit/transfer_storage.rs"]
mod tests;
