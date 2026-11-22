//! Import commands accept selected or dropped paths and run the pipeline on background
//! threads.

use crate::error::{AppError, AppResult};
use crate::services::imports::{self, ImportAccepted, PathImport};
use serde::Serialize;
const SAMPLE_IMPORT_LIMIT: usize = 10;

// Decimal MB, matching the onboarding UI: exactly 100 MB is accepted.
const FIRST_IMPORT_MAX_BYTES: u64 = 100_000_000;

/// Recovery distinguishes no new files from a job already handled by another request.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRecoveryAccepted {
    pub queued: usize,
    pub already_resolved: bool,
}

/// Import media in the background with import://progress events. Paths may mix files and
/// directories; a missing folder_id means uncategorized. Registration can run alongside other
/// batches, while processing is serialized so newly registered cards appear promptly.
#[tauri::command]
pub async fn import_paths(
    app: tauri::AppHandle,
    paths: Vec<String>,
    folder_id: Option<i64>,
    batch_id: Option<String>,
    tags: Option<Vec<String>>,
    first_import: Option<bool>,
) -> AppResult<ImportAccepted> {
    let worker_app = app.clone();
    crate::commands::blocking(app, move |state| {
        if first_import.unwrap_or(false) {
            validate_first_import_files(&paths)?;
        }
        imports::submit(
            worker_app,
            state,
            PathImport {
                paths,
                folder_id,
                batch_id,
                tags,
                ..Default::default()
            },
        )
    })
    .await
}

fn ensure_first_import_size(incoming_bytes: u64) -> AppResult<()> {
    if incoming_bytes > FIRST_IMPORT_MAX_BYTES {
        return Err(AppError::FirstImportSizeExceeded {
            incoming_bytes,
            limit_bytes: FIRST_IMPORT_MAX_BYTES,
        });
    }
    Ok(())
}

/// Check the entire selection before creating an import job, copying, or indexing.
fn validate_first_import_files(paths: &[String]) -> AppResult<u64> {
    if paths.is_empty() || paths.len() > SAMPLE_IMPORT_LIMIT {
        return Err(AppError::coded(
            "first_import_count_limit",
            "select between 1 and 10 files for the first import",
        ));
    }
    let mut total = 0_u64;
    for path in paths {
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() {
            return Err(AppError::coded(
                "first_import_files_only",
                "first import accepts files only",
            ));
        }
        total = total.saturating_add(metadata.len());
    }
    ensure_first_import_size(total)?;
    Ok(total)
}

#[cfg(test)]
#[path = "../../tests/unit/first_import.rs"]
mod first_import_tests;

mod folder_import;
mod recovery;

pub use folder_import::*;
pub use recovery::*;
