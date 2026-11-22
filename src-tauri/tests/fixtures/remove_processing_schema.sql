-- Restore the pre-v20 shape when a test starts from the current schema.
DROP TRIGGER processing_asset_trash;
DROP TRIGGER processing_asset_delete;
DROP TABLE processing_items;
DROP TABLE processing_state;
DROP TABLE processing_history;
DROP TABLE processing_tasks;
ALTER TABLE index_tasks DROP COLUMN origin;
ALTER TABLE index_tasks DROP COLUMN batch_id;
