-- Empty schema produced by main ffa10cb migrations (v16), without newer tables/columns.
CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);

CREATE TABLE folders(
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                name       TEXT NOT NULL UNIQUE,
                created_at INTEGER NOT NULL
            );

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
                import_stage TEXT NOT NULL DEFAULT 'pending', uid TEXT, source_path TEXT, source_mtime INTEGER, source_missing INTEGER NOT NULL DEFAULT 0, web_source_url TEXT, web_source_title TEXT, display_name TEXT,
                UNIQUE(folder_id, file_name)
            );

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

CREATE TABLE frames(
                id       INTEGER PRIMARY KEY AUTOINCREMENT,
                asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                seq      INTEGER NOT NULL,
                rel_path TEXT,
                embedded INTEGER NOT NULL DEFAULT 0, timestamp_ms INTEGER NOT NULL DEFAULT 0, uid TEXT,
                UNIQUE(asset_id, seq)
            );

CREATE INDEX idx_assets_folder    ON assets(folder_id) WHERE deleted_at IS NULL;

CREATE INDEX idx_assets_imported  ON assets(imported_at DESC);

CREATE INDEX idx_assets_deleted   ON assets(deleted_at);

CREATE INDEX idx_frames_asset     ON frames(asset_id);

CREATE INDEX idx_asset_tags_tag   ON asset_tags(tag_id);

CREATE TABLE asset_colors(
                 asset_id INTEGER NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
                 color    TEXT NOT NULL,
                 ratio    REAL NOT NULL,
                 rank     INTEGER NOT NULL,
                 PRIMARY KEY(asset_id, color)
             );

CREATE TABLE frame_colors(
                 frame_id INTEGER NOT NULL REFERENCES frames(id) ON DELETE CASCADE,
                 color    TEXT NOT NULL,
                 ratio    REAL NOT NULL,
                 rank     INTEGER NOT NULL,
                 PRIMARY KEY(frame_id, color)
             );

CREATE INDEX idx_asset_colors_color ON asset_colors(color, asset_id);

CREATE INDEX idx_frame_colors_color ON frame_colors(color, frame_id);

CREATE UNIQUE INDEX idx_assets_uid ON assets(uid);

CREATE UNIQUE INDEX idx_frames_uid ON frames(uid);

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
               BEGIN SELECT RAISE(ABORT, 'frame uid required'); END;

CREATE INDEX idx_assets_live_imported
               ON assets(imported_at DESC, id DESC) WHERE deleted_at IS NULL;

CREATE INDEX idx_assets_live_folder_imported
               ON assets(folder_id, imported_at DESC, id DESC) WHERE deleted_at IS NULL;

CREATE TABLE import_jobs(
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

CREATE INDEX idx_import_jobs_recovery
               ON import_jobs(status, updated_at DESC);

CREATE INDEX idx_assets_reference
               ON assets(source_path) WHERE source_path IS NOT NULL;

INSERT INTO meta(key,value) VALUES('schema_version','16'),('vector_payload_version','1');
