//! Persist the import request independently of the current page and settings.
use crate::{db, error::AppResult, library::LibrarySession};

pub(super) struct Intent {
    pub policy: db::indexing::IndexPolicy,
    pub tags: Option<Vec<String>>,
}

pub(super) fn resolve(
    session: &LibrarySession,
    id: Option<&str>,
    defaults: db::indexing::IndexPolicy,
    tags: Option<Vec<String>>,
) -> AppResult<Intent> {
    let saved = id
        .map(|id| db::import_jobs::get(&session.read_db(), id))
        .transpose()?
        .flatten();
    Ok(match saved {
        Some(job) => Intent {
            policy: job.index_policy,
            tags: Some(job.tags),
        },
        None => Intent {
            policy: defaults,
            tags,
        },
    })
}

pub(super) struct Job<'a> {
    pub id: &'a str,
    pub mode: &'a str,
    pub paths: &'a [String],
    pub folder: Option<i64>,
    pub policy: db::indexing::IndexPolicy,
    pub tags: Option<&'a [String]>,
}

fn save_intent(conn: &rusqlite::Connection, job: &Job<'_>) -> AppResult<()> {
    db::import_jobs::start(conn, job.id, job.mode, job.paths, job.folder)?;
    if let Some(tags) = job.tags {
        db::import_jobs::set_tags(conn, job.id, tags)?;
    }
    conn.execute(
        "UPDATE import_jobs SET visual_index=?1,subtitle_index=?2 WHERE id=?3",
        rusqlite::params![job.policy.visual, job.policy.subtitles, job.id],
    )?;
    Ok(())
}

pub(super) fn save(session: &LibrarySession, job: Job<'_>) -> AppResult<()> {
    let conn = session.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    save_intent(&tx, &job)?;
    tx.commit()?;
    Ok(())
}

/// Publish durable handoff evidence only after quota admission, together with its queue rows.
pub(super) fn save_paths(
    session: &LibrarySession,
    job: Job<'_>,
    scanned: &[std::path::PathBuf],
) -> AppResult<()> {
    let conn = session.db.lock().unwrap();
    let tx = conn.unchecked_transaction()?;
    save_intent(&tx, &job)?;
    for path in scanned {
        db::processing::register(&tx, job.id, path, job.folder)?;
    }
    tx.commit()?;
    Ok(())
}

/// Apply the saved intent to both newly registered assets and deduplicated assets.
pub(super) fn apply_tags(
    session: &LibrarySession,
    batch: &crate::import::RegisteredBatch,
    tags: Option<&[String]>,
) {
    if let Some(tags) = tags.filter(|tags| !tags.is_empty()) {
        let conn = session.db.lock().unwrap();
        for asset_id in batch
            .pending
            .iter()
            .map(|(id, _)| *id)
            .chain(batch.duplicates.iter().copied())
        {
            if let Err(error) = crate::db::tags::add_to_asset(&conn, asset_id, tags) {
                crate::logging::warn(
                    "capture",
                    "tag_apply_failed",
                    "-",
                    &format!("asset_id={asset_id};message={error}"),
                );
            }
        }
    }
    if tags.is_some_and(|tags| !tags.is_empty()) {
        if let Err(error) = crate::vector::sync_assets_payload(session, &batch.duplicates) {
            crate::logging::warn("import", "tag_sync_failed", "-", &error.to_string());
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/import_context.rs"]
mod tests;
