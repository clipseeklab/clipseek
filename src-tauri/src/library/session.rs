use super::{check_compatible, LibraryDirs, LibrarySession, VectorHealth};
use crate::config::LibraryMeta;
use crate::error::{AppError, AppResult};
use std::path::PathBuf;
use std::sync::{atomic::AtomicUsize, Mutex};

/// Open a session with database and directory validation.
pub fn open_session(meta: LibraryMeta) -> AppResult<LibrarySession> {
    open_session_with_mode(meta, false, false)
}

/// Open a read-only session without migrations or write recovery for callers that only
/// inspect an existing format.
pub fn open_session_read_only(meta: LibraryMeta) -> AppResult<LibrarySession> {
    open_session_with_mode(meta, true, false)
}

pub fn open_session_light(meta: LibraryMeta) -> AppResult<LibrarySession> {
    open_session_with_mode(meta, false, true)
}

/// Acquire exclusive opening access before closing the previous session and upgrading.
pub(crate) fn open_session_with_close(
    meta: LibraryMeta,
    close_session: impl FnOnce(),
) -> AppResult<LibrarySession> {
    open_session_with_mode_and_close(meta, false, false, close_session)
}

fn open_session_with_mode(
    meta: LibraryMeta,
    read_only: bool,
    lazy_vector: bool,
) -> AppResult<LibrarySession> {
    open_session_with_mode_and_close(meta, read_only, lazy_vector, || {})
}

fn open_session_with_mode_and_close(
    meta: LibraryMeta,
    read_only: bool,
    lazy_vector: bool,
    close_session: impl FnOnce(),
) -> AppResult<LibrarySession> {
    let root = PathBuf::from(&meta.path);
    if !root.join(".clipstudio-open").is_dir() {
        return Err(AppError::coded(
            "library_dir_invalid",
            format!(
                "not a valid library directory (missing .clipstudio): {}",
                meta.path
            ),
        ));
    }
    let asset_activity = super::activity::for_root(&root)?;
    let _opening = asset_activity.cleanup()?;
    close_session();
    let dirs = LibraryDirs::new(&root);
    check_compatible(&root)?;
    let vector_recovery_error = if !read_only {
        match crate::vector::recovery::recover_interrupted_swap(&root) {
            Ok(recovery) => {
                if recovery.restored_backup {
                    crate::logging::warn(
                        "vector",
                        "interrupted_swap_rolled_back",
                        &meta.id.chars().take(8).collect::<String>(),
                        "action=restore_backup;rebuild_started=false",
                    );
                }
                None
            }
            Err(error) => {
                let message = error.to_string().replace(&meta.path, "<library>");
                crate::logging::error(
                    "vector",
                    "interrupted_swap_recovery_failed",
                    &meta.id.chars().take(8).collect::<String>(),
                    &format!("message={message};rebuild_started=false"),
                );
                Some(message)
            }
        }
    } else {
        None
    };
    let db_path = dirs.db();
    if read_only && !db_path.is_file() {
        return Err(AppError::coded(
            "library_database_open_failed",
            format!("database file does not exist: {}", db_path.display()),
        ));
    }
    let conn = if read_only {
        crate::db::open_library_read_db(&db_path)
    } else {
        crate::db::open_library_db(&db_path)
    }
    .map_err(|error| {
        if matches!(
            error.code(),
            "library_upgrade_required"
                | "library_version_invalid"
                | "library_migration_backup_failed"
        ) {
            return error;
        }
        AppError::coded(
            "library_database_open_failed",
            format!("failed to open database {}: {error}", db_path.display()),
        )
    })?;
    if read_only
        && conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM meta WHERE key LIKE 'asset_lifecycle.v1.%')",
            [],
            |r| r.get::<_, bool>(0),
        )?
    {
        return Err(AppError::coded(
            "asset_rollback_failed",
            "Open the library with write access to recover interrupted file moves",
        ));
    }
    let read_dbs = [
        Mutex::new(crate::db::open_library_read_db(&db_path).map_err(|error| {
            AppError::coded(
                "library_database_open_failed",
                format!(
                    "failed to open read connection {}: {error}",
                    db_path.display()
                ),
            )
        })?),
        Mutex::new(crate::db::open_library_read_db(&db_path).map_err(|error| {
            AppError::coded(
                "library_database_open_failed",
                format!(
                    "failed to open read connection {}: {error}",
                    db_path.display()
                ),
            )
        })?),
    ];
    // Index-open failure must not block browsing or session creation.
    let vector_started = std::time::Instant::now();
    let library_operation = meta.id.chars().take(8).collect::<String>();
    crate::logging::info(
        "vector",
        "open_started",
        &library_operation,
        &crate::vector::index_storage_summary(&dirs.vector()),
    );
    let (vector, vector_health) = if let Some(message) = vector_recovery_error {
        (
            None,
            VectorHealth::RebuildPending {
                code: "vector_rebuild_recovery_failed".into(),
                message,
            },
        )
    } else if lazy_vector {
        (None, VectorHealth::Ready)
    } else {
        match crate::vector::open_index(&dirs.vector()) {
            Ok(shard) => {
                crate::logging::info(
                    "vector",
                    "open_succeeded",
                    &library_operation,
                    &format!(
                        "elapsed_ms={};{}",
                        vector_started.elapsed().as_millis(),
                        crate::vector::index_storage_summary(&dirs.vector())
                    ),
                );
                (Some(shard), VectorHealth::Ready)
            }
            Err(e) => {
                let safe_error = e.to_string().replace(&meta.path, "<library>");
                crate::logging::error(
                    "vector",
                    "open_failed",
                    &library_operation,
                    &format!(
                        "elapsed_ms={};message={safe_error};{}",
                        vector_started.elapsed().as_millis(),
                        crate::vector::index_storage_summary(&dirs.vector())
                    ),
                );
                eprintln!("Vector index could not open (search unavailable): {e}");
                (
                    None,
                    VectorHealth::RebuildPending {
                        code: "vector_index_open_failed".into(),
                        message: safe_error,
                    },
                )
            }
        }
    };
    let session = LibrarySession {
        meta,
        root,
        db: Mutex::new(conn),
        asset_activity: asset_activity.clone(),
        index_worker_running: std::sync::atomic::AtomicBool::new(false),
        read_dbs,
        next_read_db: AtomicUsize::new(0),
        vector: Mutex::new(vector),
        vector_health: Mutex::new(vector_health),
        vector_rebuild_cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    if !read_only {
        crate::services::assets::recover_file_moves_on_open(&session)?;

        crate::db::processing::recover(&session.db.lock().unwrap())?;

        crate::db::indexing::set_status(
            &session.db.lock().unwrap(),
            None,
            "queued",
            None,
            &["running"],
        )?;
        crate::db::processing::maintain(&session.db.lock().unwrap(), crate::util::now())?;
    }
    Ok(session)
}
