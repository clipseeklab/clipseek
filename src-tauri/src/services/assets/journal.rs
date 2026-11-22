//! Write-ahead file moves. SQLite's commit marker decides rollback versus retention.
use std::fs;
use std::path::{Component, Path};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use super::file_moves::{move_without_replace, stage_path, sync_parent};
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use crate::{util, vector};

const PREFIX: &str = "asset_lifecycle.v1.";

#[derive(Serialize, Deserialize)]
pub(super) struct Move {
    pub source: String,
    pub target: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Journal {
    pub committed: bool,
    pub ids: Vec<i64>,
    pub moves: Vec<Move>,
}

/// NORMAL is sufficient for ordinary writes, but file moves need a durable intent
/// before removing any source and a durable commit marker before journal removal.
pub(super) struct FullSync<'a>(&'a Connection, i64);
impl<'a> FullSync<'a> {
    pub fn new(conn: &'a Connection) -> AppResult<Self> {
        let previous = conn.pragma_query_value(None, "synchronous", |r| r.get(0))?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        Ok(Self(conn, previous))
    }
}
impl Drop for FullSync<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.0.pragma_update(None, "synchronous", self.1) {
            tracing::error!(%error, "could not restore SQLite synchronization mode");
        }
    }
}

impl Journal {
    pub fn save(&self, conn: &Connection, key: &str) -> AppResult<()> {
        conn.execute("INSERT INTO meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![key, serde_json::to_string(self)?])?;
        Ok(())
    }
    pub fn key() -> String {
        format!("{PREFIX}{}", util::new_id())
    }
    pub fn clear(conn: &Connection, key: &str) -> AppResult<()> {
        conn.execute("DELETE FROM meta WHERE key=?1", [key])?;
        Ok(())
    }
    pub fn plan(root: &Path, source: &Path, target: &Path) -> AppResult<Move> {
        validate(root, source)?;
        validate(root, target)?;
        for stage in [stage_path(source, target), stage_path(target, source)] {
            if stage.try_exists()? {
                return Err(conflict(&stage));
            }
        }
        #[cfg(unix)]
        fs::File::open(source)?.sync_all()?;
        let mut directory = target.parent();
        while let Some(path) = directory.filter(|p| p.starts_with(root)) {
            sync_parent(path)?;
            if path == root {
                break;
            }
            directory = path.parent();
        }
        Ok(Move {
            source: util::to_rel(root, source),
            target: util::to_rel(root, target),
            hash: hash(source)?,
        })
    }
}

fn conflict(path: &Path) -> AppError {
    AppError::coded(
        "asset_rollback_failed",
        format!(
            "File recovery needs attention; preserved {}",
            path.display()
        ),
    )
}

fn hash(path: &Path) -> AppResult<String> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(conflict(path));
    }
    let mut input = fs::File::open(path)?;
    let mut digest = blake3::Hasher::new();
    std::io::copy(&mut input, &mut digest)?;
    Ok(digest.finalize().to_hex().to_string())
}

fn validate(root: &Path, path: &Path) -> AppResult<()> {
    let rel = path.strip_prefix(root).map_err(|_| conflict(path))?;
    if rel.components().any(|p| !matches!(p, Component::Normal(_)))
        || !(rel.starts_with("images") || rel.starts_with(".clipstudio-open/trash"))
    {
        return Err(conflict(path));
    }
    let mut existing = path;
    while !existing.try_exists()? {
        existing = existing.parent().ok_or_else(|| conflict(path))?;
    }
    if !existing.canonicalize()?.starts_with(root.canonicalize()?) {
        return Err(conflict(path));
    }
    if fs::symlink_metadata(existing)?.file_type().is_symlink() {
        return Err(conflict(path));
    }
    Ok(())
}

/// Caller owns exclusive library activity. Run before exposing paths to consumers.
pub(crate) fn recover(session: &LibrarySession) -> AppResult<()> {
    recover_inner(session, false)
}

/// File consistency is mandatory on open. A post-commit vector error must leave
/// the library accessible so the user can rebuild the damaged search index.
pub(crate) fn recover_on_open(session: &LibrarySession) -> AppResult<()> {
    recover_inner(session, true)
}

fn recover_inner(session: &LibrarySession, defer_vectors: bool) -> AppResult<()> {
    let records = {
        let conn = session.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT key,value FROM meta WHERE key LIKE 'asset_lifecycle.v1.%' ORDER BY key",
        )?;
        let records = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        records
    };
    if !records.is_empty() {
        session.asset_activity.set_recovery_required(true);
    }
    let mut pending_vectors = Vec::new();
    for (key, value) in records {
        let mut journal: Journal = serde_json::from_str(&value).map_err(|e| {
            AppError::coded(
                "asset_rollback_failed",
                format!("Invalid file recovery journal {key}: {e}"),
            )
        })?;
        for entry in journal.moves.iter().rev() {
            recover_move(&session.root, entry, journal.committed)?;
        }
        let conn = session.db.lock().unwrap();
        let _sync = FullSync::new(&conn)?;
        if journal.committed {
            // Files are consistent. Future edits must not be mistaken for old
            // rollback conflicts; keep only the post-commit payload-sync intent.
            journal.moves.clear();
            journal.save(&conn, &key)?;
            pending_vectors.push((key, journal.ids));
        } else {
            Journal::clear(&conn, &key)?;
        }
    }
    // Index damage must not block previewing files or starting an index repair.
    session.asset_activity.set_recovery_required(false);
    for (key, ids) in pending_vectors {
        if let Err(error) = vector::sync_assets_payload(session, &ids) {
            if !defer_vectors {
                return Err(error);
            }
            tracing::warn!(%error, "asset files recovered; vector payload sync deferred until repair");
            continue;
        }
        let conn = session.db.lock().unwrap();
        let _sync = FullSync::new(&conn)?;
        Journal::clear(&conn, &key)?;
    }
    Ok(())
}

fn recover_move(root: &Path, entry: &Move, committed: bool) -> AppResult<()> {
    let source = root.join(&entry.source);
    let target = root.join(&entry.target);
    validate(root, &source)?;
    validate(root, &target)?;
    let (keep, discard) = if committed {
        (&target, &source)
    } else {
        (&source, &target)
    };
    let keep_exists = keep.try_exists()?;
    let discard_exists = discard.try_exists()?;
    if keep_exists && hash(keep)? != entry.hash {
        return Err(conflict(keep));
    }
    if discard_exists && hash(discard)? != entry.hash {
        return Err(conflict(discard));
    }
    if !keep_exists {
        if !discard_exists {
            return Err(conflict(keep));
        }
        fs::create_dir_all(keep.parent().unwrap())?;
        // Remove an interrupted copy only after verifying an intact original.
        clear_stage(&stage_path(discard, keep))?;
        move_without_replace(discard, keep)?;
    } else if discard_exists {
        fs::remove_file(discard)?;
        sync_parent(discard)?;
    }
    for stage in [stage_path(&source, &target), stage_path(&target, &source)] {
        clear_stage(&stage)?;
    }
    Ok(())
}

fn clear_stage(path: &Path) -> AppResult<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => {
            fs::remove_file(path)?;
            sync_parent(path)
        }
        Ok(_) => Err(conflict(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/asset_move_journal.rs"]
mod tests;
