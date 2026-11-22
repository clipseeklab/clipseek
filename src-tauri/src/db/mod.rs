pub mod assets;
pub mod folders;
pub mod import_jobs;
pub mod indexing;
pub mod model;
pub mod processing;
pub(crate) mod search;
pub mod tags;

// Preserve the existing DAO path after grouping asset persistence in one directory.
pub use assets::asset_cleanup;

use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use crate::error::{AppError, AppResult};

pub const SCHEMA_VERSION: i64 = 24;

/// Read before WAL setup, migrations, and derivative recovery.
pub fn check_compatible(db_path: &Path) -> AppResult<()> {
    if !db_path.is_file() {
        return Ok(());
    }
    let conn = Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    check_connection(&conn)
}

fn check_connection(conn: &Connection) -> AppResult<()> {
    let has_meta: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='meta')",
        [],
        |r| r.get(0),
    )?;
    if !has_meta {
        return Ok(());
    }
    let version: Option<String> = conn
        .query_row(
            "SELECT CAST(value AS TEXT) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(version) = version {
        let version: i64 = version.parse().map_err(|_| {
            AppError::coded("library_version_invalid", "Invalid database schema version")
        })?;
        if version < 0 {
            return Err(AppError::coded(
                "library_version_invalid",
                "Invalid negative schema version",
            ));
        }
        if version > SCHEMA_VERSION {
            return Err(AppError::coded("library_upgrade_required", format!("Library schema {version} exceeds supported schema {SCHEMA_VERSION}; upgrade ClipSeek")));
        }
    }
    Ok(())
}

use rusqlite::OptionalExtension;

/// Open or create the library database, enable foreign keys and WAL, and run schema
/// migrations.
pub fn open_library_db(db_path: &Path) -> AppResult<Connection> {
    check_compatible(db_path)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut conn = Connection::open(db_path)?;
    check_connection(&conn)?;
    // Keep a consistent pre-upgrade SQLite snapshot, including any committed WAL data.
    let old_version: Option<i64> = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |r| r.get(0),
        )
        .optional()
        .unwrap_or(None);
    if old_version.is_some_and(|version| version > 0 && version < SCHEMA_VERSION) {
        let backup_dir = db_path.parent().unwrap_or(Path::new(".")).join("backups");
        std::fs::create_dir_all(&backup_dir)?;
        let backup = backup_dir.join(format!(
            "library-v{}-{}.db",
            old_version.unwrap(),
            crate::util::new_id()
        ));
        if let Err(error) = conn.execute("VACUUM INTO ?1", [backup.to_string_lossy().as_ref()]) {
            let _ = std::fs::remove_file(backup);
            return Err(AppError::coded(
                "library_migration_backup_failed",
                format!("Could not back up the library before migration: {error}"),
            ));
        }
    }
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "cache_size", -32_768i64)?;
    conn.pragma_update(None, "mmap_size", 268_435_456i64)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    migrate(&mut conn)?;
    Ok(conn)
}

/// Open a resident read-only WAL connection without migrations or write transactions.
pub fn open_library_read_db(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    check_connection(&conn)?;
    conn.pragma_update(None, "query_only", "ON")?;
    conn.pragma_update(None, "cache_size", -16_384i64)?;
    conn.pragma_update(None, "mmap_size", 268_435_456i64)?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    Ok(conn)
}

/// Run versioned DDL using schema_version in meta; append a migration when adding tables or
/// columns.
fn migrate(conn: &mut Connection) -> AppResult<()> {
    check_connection(conn)?;
    let tx = conn.transaction()?;
    migrate_in_transaction(&tx)?;
    tx.commit()?;
    Ok(())
}

fn migrate_in_transaction(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    // The value column has TEXT affinity; accept both integer and text representations.
    let current: i64 = conn
        .query_row(
            "SELECT value FROM meta WHERE key='schema_version'",
            [],
            |r| {
                let v: rusqlite::types::Value = r.get(0)?;
                Ok(match v {
                    rusqlite::types::Value::Integer(i) => i,
                    rusqlite::types::Value::Text(s) => s.parse().unwrap_or(0),
                    _ => 0,
                })
            },
        )
        .unwrap_or(0);

    if current < 1 {
        conn.execute_batch(
            r#"
            -- Logical folders, independent of physical asset paths
            CREATE TABLE folders(
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL
            );

            -- Asset files
            -- NULL folder_id means uncategorized; derived files use images/YY-MM/asset-UUID
            -- A non-NULL deleted_at marks trash; referenced source files remain unchanged
            -- import_stage: pending -> copied -> probed -> thumb -> frames -> embedded -> done / error:<msg>
            CREATE TABLE assets(
                id           INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id    INTEGER REFERENCES folders(id) ON DELETE SET NULL,
                file_name    TEXT NOT NULL,
                rel_path     TEXT NOT NULL,
                file_hash    TEXT NOT NULL UNIQUE,
                file_size    INTEGER NOT NULL,
                mime_type    TEXT NOT NULL,
                kind         TEXT NOT NULL,
                width        INTEGER,
                height       INTEGER,
                duration_ms  INTEGER,
                color        TEXT,
                thumb_rel    TEXT,
                imported_at  INTEGER NOT NULL,
                deleted_at   INTEGER,
                import_stage TEXT NOT NULL DEFAULT 'pending',
                UNIQUE(folder_id, file_name)
            );

            -- Tags
            CREATE TABLE tags(
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL
            );
            CREATE TABLE asset_tags(
                asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY(asset_id, tag_id)
            );

            -- Internal index units; current media has one sequence-zero unit
            -- frames.id is the vector point ID; embedded records persistence in the vector index
            CREATE TABLE frames(
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                seq      INTEGER NOT NULL,
                rel_path TEXT,
                embedded INTEGER NOT NULL DEFAULT 0,
                UNIQUE(asset_id, seq)
            );

            CREATE INDEX idx_assets_folder    ON assets(folder_id) WHERE deleted_at IS NULL;
            CREATE INDEX idx_assets_imported  ON assets(imported_at DESC);
            CREATE INDEX idx_assets_deleted   ON assets(deleted_at);
            CREATE INDEX idx_frames_asset     ON frames(asset_id);
            CREATE INDEX idx_asset_tags_tag   ON asset_tags(tag_id);
            "#,
        )?;
    }

    if current < 2 {
        conn.execute_batch(
            "ALTER TABLE frames ADD COLUMN timestamp_ms INTEGER NOT NULL DEFAULT 0;
             UPDATE assets SET import_stage = 'reindex' WHERE kind IN ('video', 'gif');",
        )?;
    }

    // Legacy v3 switched I-frame output to -vsync vfr and rebuilt derived inputs.
    if current < 3 {
        conn.execute(
            "UPDATE assets SET import_stage = 'reindex' WHERE kind IN ('video', 'gif')",
            [],
        )?;
    }

    // Legacy v4 renamed frame files to asset-ID/timestamp names and removed extra ffprobe
    // scans; v11 later switched directories to UUIDs.
    if current < 4 {
        conn.execute(
            "UPDATE assets SET import_stage = 'reindex' WHERE kind IN ('video', 'gif')",
            [],
        )?;
    }

    // Legacy v5 regenerated frame and video previews as lossy WebP to reduce disk usage.
    if current < 5 {
        conn.execute(
            "UPDATE assets SET import_stage = 'reindex' WHERE kind IN ('video', 'gif')",
            [],
        )?;
    }

    // Legacy v6 removed orphan video/GIF vectors and rebuilt their previews. Existing static
    // images were not forcibly rebuilt to preserve availability in large libraries.
    if current < 6 {
        conn.execute(
            "UPDATE assets SET import_stage = 'reindex' WHERE kind IN ('video', 'gif')",
            [],
        )?;
    }

    // Legacy v7 indexed GIFs as one static thumbnail and removed their video proxies.
    if current < 7 {
        conn.execute(
            "UPDATE assets SET import_stage = 'reindex' WHERE kind = 'gif'",
            [],
        )?;
    }

    // Version 8 reserves a schema slot; recent tags are cached by the client on exit.
    if current < 8 {
        // No database schema changes.
    }

    // Version 9 stores up to three representative colors and their proportions per cover and
    // search unit.
    if current < 9 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS asset_colors(
                 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                 color    TEXT NOT NULL,
                 ratio    REAL NOT NULL,
                 rank     INTEGER NOT NULL,
                 PRIMARY KEY(asset_id, color)
             );
             CREATE TABLE IF NOT EXISTS frame_colors(
                 frame_id INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
                 color    TEXT NOT NULL,
                 ratio    REAL NOT NULL,
                 rank     INTEGER NOT NULL,
                 PRIMARY KEY(frame_id, color)
             );
             CREATE INDEX IF NOT EXISTS idx_asset_colors_color ON asset_colors(color, asset_id);
             CREATE INDEX IF NOT EXISTS idx_frame_colors_color ON frame_colors(color, frame_id);",
        )?;
    }

    // Version 10 adds aspect to vector payloads; synchronize existing metadata without
    // rerunning the encoder.
    if current < 10 {
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('vector_payload_version', '1')
             ON CONFLICT(key) DO UPDATE SET value='1'",
            [],
        )?;
    }

    // Version 11 keeps efficient integer primary keys and adds UUIDv7 cross-library
    // identities.
    if current < 11 {
        conn.execute_batch(
            "ALTER TABLE assets ADD COLUMN uid TEXT; ALTER TABLE frames ADD COLUMN uid TEXT;",
        )?;
        let asset_ids = {
            let mut stmt = conn.prepare("SELECT id FROM assets WHERE uid IS NULL")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in asset_ids {
            conn.execute(
                "UPDATE assets SET uid=?1 WHERE id=?2",
                rusqlite::params![crate::util::new_entity_uid(), id],
            )?;
        }
        let frame_ids = {
            let mut stmt = conn.prepare("SELECT id FROM frames WHERE uid IS NULL")?;
            let rows = stmt
                .query_map([], |r| r.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in frame_ids {
            conn.execute(
                "UPDATE frames SET uid=?1 WHERE id=?2",
                rusqlite::params![crate::util::new_entity_uid(), id],
            )?;
        }
        conn.execute_batch(
            "CREATE UNIQUE INDEX idx_assets_uid ON assets(uid);
             CREATE UNIQUE INDEX idx_frames_uid ON frames(uid);
             CREATE TRIGGER assets_uid_required BEFORE INSERT ON assets WHEN NEW.uid IS NULL BEGIN SELECT RAISE(ABORT, 'asset uid required'); END;
             CREATE TRIGGER frames_uid_required BEFORE INSERT ON frames WHEN NEW.uid IS NULL BEGIN SELECT RAISE(ABORT, 'frame uid required'); END;"
        )?;
    }

    // Version 12 rejects empty UIDs on inserts and updates.
    if current < 12 {
        conn.execute_batch(
            "DROP TRIGGER IF EXISTS assets_uid_required;
             DROP TRIGGER IF EXISTS frames_uid_required;
             CREATE TRIGGER assets_uid_required BEFORE INSERT ON assets
               WHEN NEW.uid IS NULL OR length(trim(NEW.uid))=0
               BEGIN SELECT RAISE(ABORT, 'asset uid required'); END;
             CREATE TRIGGER frames_uid_required BEFORE INSERT ON frames
               WHEN NEW.uid IS NULL OR length(trim(NEW.uid))=0
               BEGIN SELECT RAISE(ABORT, 'frame uid required'); END;
             CREATE TRIGGER assets_uid_update_required BEFORE UPDATE OF uid ON assets
               WHEN NEW.uid IS NULL OR length(trim(NEW.uid))=0
               BEGIN SELECT RAISE(ABORT, 'asset uid required'); END;
             CREATE TRIGGER frames_uid_update_required BEFORE UPDATE OF uid ON frames
               WHEN NEW.uid IS NULL OR length(trim(NEW.uid))=0
               BEGIN SELECT RAISE(ABORT, 'frame uid required'); END;",
        )?;
    }

    // Version 13 indexes stable default and folder ordering to avoid temporary B-trees.
    if current < 13 {
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_assets_live_imported
               ON assets(imported_at DESC, id DESC) WHERE deleted_at IS NULL;
             CREATE INDEX IF NOT EXISTS idx_assets_live_folder_imported
               ON assets(folder_id, imported_at DESC, id DESC) WHERE deleted_at IS NULL;",
        )?;
    }

    // Version 14 persists import jobs so interrupted queued or running jobs can be resumed.
    if current < 14 {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS import_jobs(
                 id          TEXT PRIMARY KEY,
                 mode        TEXT NOT NULL,
                 paths_json  TEXT NOT NULL,
                 folder_id   INTEGER,
                 status      TEXT NOT NULL,
                 total       INTEGER NOT NULL DEFAULT 0,
                 done        INTEGER NOT NULL DEFAULT 0,
                 failed      INTEGER NOT NULL DEFAULT 0,
                 duplicates  INTEGER NOT NULL DEFAULT 0,
                 created_at  INTEGER NOT NULL,
                 updated_at  INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS idx_import_jobs_recovery
               ON import_jobs(status, updated_at DESC);",
        )?;
    }

    // Version 15 adds reference sources, modification times, and missing-source flags; legacy
    // managed assets have no source_path.
    if current < 15 {
        conn.execute_batch(
            "ALTER TABLE assets ADD COLUMN source_path TEXT;
             ALTER TABLE assets ADD COLUMN source_mtime INTEGER;
             ALTER TABLE assets ADD COLUMN source_missing INTEGER NOT NULL DEFAULT 0;
             CREATE INDEX idx_assets_reference
               ON assets(source_path) WHERE source_path IS NOT NULL;",
        )?;
    }

    // Version 16 retains legacy web provenance and editable display titles; empty
    // display_name falls back to file_name.
    if current < 16 {
        for column in ["web_source_url", "web_source_title", "display_name"] {
            let exists: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('assets') WHERE name=?1)",
                [column],
                |row| row.get(0),
            )?;
            if !exists {
                conn.execute(&format!("ALTER TABLE assets ADD COLUMN {column} TEXT"), [])?;
            }
        }
    }

    if current < 18 {
        conn.execute(
            "ALTER TABLE assets ADD COLUMN cover_manual INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
        // Before v18, only a manually selected video cover pointed directly at a frame.
        let has_frame_paths: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM pragma_table_info('assets') WHERE name='thumb_rel') AND EXISTS(SELECT 1 FROM pragma_table_info('frames') WHERE name='rel_path')",
            [], |row| row.get(0),
        )?;
        if has_frame_paths {
            conn.execute("UPDATE assets SET cover_manual=1 WHERE kind='video' AND EXISTS(SELECT 1 FROM frames WHERE frames.asset_id=assets.id AND frames.rel_path=assets.thumb_rel)", [])?;
        }
    }

    if current < 19 {
        legacy_metadata_schema(conn)?;
        indexing::schema(conn)?;
    }
    if current < 20 {
        processing::schema(conn)?;
    }
    // Older releases did not reject newer schemas and always wrote their own version back.
    // Stop their legacy UPSERT as well, rather than relying solely on new application code.
    conn.execute_batch(
        "CREATE TRIGGER IF NOT EXISTS schema_version_no_downgrade
        BEFORE UPDATE OF value ON meta
        WHEN OLD.key='schema_version' AND CAST(NEW.value AS INTEGER)<CAST(OLD.value AS INTEGER)
        BEGIN SELECT RAISE(ABORT,'Please upgrade ClipSeek Open before opening this library'); END;",
    )?;
    conn.execute(
        "INSERT INTO meta(key, value) VALUES('schema_version', 24)
         ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        [],
    )?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/db_meta.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/db_migrations.rs"]
mod migration_regression_tests;

fn legacy_metadata_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch("CREATE TABLE IF NOT EXISTS subtitle_tracks(asset_id INTEGER PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,version TEXT NOT NULL,source_name TEXT NOT NULL,encoding TEXT NOT NULL,raw BLOB NOT NULL,warnings TEXT NOT NULL);
 CREATE TABLE IF NOT EXISTS subtitle_cues(id INTEGER PRIMARY KEY AUTOINCREMENT,asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,version TEXT NOT NULL,seq INTEGER NOT NULL,start_ms INTEGER NOT NULL,end_ms INTEGER NOT NULL,text TEXT NOT NULL,UNIQUE(asset_id,version,seq));
 CREATE TABLE IF NOT EXISTS subtitle_docs(id INTEGER PRIMARY KEY AUTOINCREMENT,asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,version TEXT NOT NULL,seq INTEGER NOT NULL,end_seq INTEGER NOT NULL,text TEXT NOT NULL,UNIQUE(asset_id,version,seq));
 CREATE VIRTUAL TABLE IF NOT EXISTS subtitle_words USING fts5(text,content='',contentless_delete=1,tokenize='porter unicode61 remove_diacritics 0');
 CREATE VIRTUAL TABLE IF NOT EXISTS subtitle_grams USING fts5(text,content='',contentless_delete=1,detail=none);
 CREATE TRIGGER IF NOT EXISTS subtitle_doc_delete AFTER DELETE ON subtitle_docs BEGIN DELETE FROM subtitle_words WHERE rowid=old.id; DELETE FROM subtitle_grams WHERE rowid=old.id; END;
 CREATE TABLE IF NOT EXISTS subtitle_builds(asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,version TEXT PRIMARY KEY);
 CREATE TABLE IF NOT EXISTS import_notices(id INTEGER PRIMARY KEY AUTOINCREMENT,asset_id INTEGER REFERENCES assets(id) ON DELETE CASCADE,file_name TEXT NOT NULL,message TEXT NOT NULL);")?;
    Ok(())
}
