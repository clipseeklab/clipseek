-- Unreleased schema 20: current steps and past attempts have separate lifetimes.
ALTER TABLE index_tasks ADD COLUMN origin TEXT NOT NULL DEFAULT 'automatic';
ALTER TABLE index_tasks ADD COLUMN batch_id TEXT;

CREATE TABLE processing_tasks(
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    item_key TEXT NOT NULL, batch_id TEXT NOT NULL,
    asset_id INTEGER REFERENCES assets(id) ON DELETE SET NULL,
    file_name TEXT NOT NULL, source_path TEXT NOT NULL,
    kind TEXT NOT NULL, status TEXT NOT NULL, phase TEXT NOT NULL,
    error TEXT, attempt INTEGER NOT NULL DEFAULT 1,
    index_task_id INTEGER, notice_id INTEGER, target_folder_id INTEGER,
    hidden INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
    group_key TEXT NOT NULL
);
CREATE UNIQUE INDEX processing_current_kind ON processing_tasks(group_key,kind) WHERE kind<>'warning';
CREATE INDEX processing_group ON processing_tasks(group_key,kind,id);
CREATE INDEX processing_item ON processing_tasks(item_key,kind);
CREATE INDEX processing_asset ON processing_tasks(asset_id,kind);
CREATE INDEX processing_batch ON processing_tasks(batch_id,status);
CREATE INDEX processing_index ON processing_tasks(index_task_id);
CREATE INDEX processing_notice ON processing_tasks(notice_id);
CREATE INDEX processing_expiry ON processing_tasks(updated_at,id) WHERE status NOT IN ('queued','running','attention');

CREATE TABLE processing_history(
    id INTEGER PRIMARY KEY ,
    item_key TEXT NOT NULL, batch_id TEXT NOT NULL,
    asset_id INTEGER REFERENCES assets(id) ON DELETE SET NULL,
    file_name TEXT NOT NULL, source_path TEXT NOT NULL,
    kind TEXT NOT NULL, status TEXT NOT NULL, phase TEXT NOT NULL,
    error TEXT, attempt INTEGER NOT NULL DEFAULT 1,
    index_task_id INTEGER, notice_id INTEGER, target_folder_id INTEGER,
    hidden INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
    group_key TEXT NOT NULL
);
CREATE INDEX processing_history_group ON processing_history(group_key,id DESC);
CREATE INDEX processing_history_expiry ON processing_history(updated_at,id);
CREATE INDEX processing_history_asset ON processing_history(asset_id,kind);

CREATE TABLE processing_items(
    group_key TEXT PRIMARY KEY,
    batch_id TEXT NOT NULL, asset_id INTEGER REFERENCES assets(id) ON DELETE SET NULL,
    file_name TEXT NOT NULL, source_path TEXT NOT NULL,
    created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL,
    sort_id INTEGER NOT NULL, status TEXT NOT NULL,
    active INTEGER NOT NULL, kind_mask INTEGER NOT NULL,
    import_id INTEGER, visual_id INTEGER, subtitles_id INTEGER, warning_id INTEGER,
    warning_count INTEGER NOT NULL
);
CREATE INDEX processing_items_status ON processing_items(status,updated_at DESC,sort_id DESC);
CREATE INDEX processing_items_recent ON processing_items(updated_at DESC,sort_id DESC);
CREATE TABLE processing_state(
    id INTEGER PRIMARY KEY CHECK(id=1), revision INTEGER NOT NULL DEFAULT 0,
    last_cleanup_at INTEGER NOT NULL DEFAULT 0
);
INSERT INTO processing_state(id) VALUES(1);

-- Lifecycle protection also covers direct SQL deletion and foreign-key cascades.
-- Stable group keys preserve file history after the material itself is deleted.
CREATE TRIGGER processing_asset_trash AFTER UPDATE OF deleted_at ON assets WHEN NEW.deleted_at IS NOT NULL
BEGIN
    UPDATE processing_tasks SET status='cancelled',updated_at=unixepoch()
    WHERE asset_id=NEW.id AND status IN ('queued','running');
    UPDATE processing_state SET revision=revision+1 WHERE id=1
        AND EXISTS(SELECT 1 FROM processing_items WHERE asset_id=NEW.id);
    UPDATE processing_items SET
        status=CASE WHEN active=1 AND status<>'attention' THEN 'cancelled' ELSE status END,
        updated_at=CASE WHEN active=1 THEN unixepoch() ELSE updated_at END,active=0
    WHERE asset_id=NEW.id;
END;
CREATE TRIGGER processing_asset_delete BEFORE DELETE ON assets
BEGIN
    UPDATE processing_tasks SET status='cancelled',updated_at=unixepoch()
    WHERE asset_id=OLD.id AND status IN ('queued','running');
    UPDATE processing_state SET revision=revision+1 WHERE id=1
        AND EXISTS(SELECT 1 FROM processing_items WHERE asset_id=OLD.id);
    UPDATE processing_items SET
        status=CASE WHEN active=1 AND status<>'attention' THEN 'cancelled' ELSE status END,
        updated_at=CASE WHEN active=1 THEN unixepoch() ELSE updated_at END,active=0
    WHERE asset_id=OLD.id;
END;
