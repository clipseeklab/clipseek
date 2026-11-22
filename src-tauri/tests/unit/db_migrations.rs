use super::*;

#[test]
fn v18_preserves_legacy_manual_covers_and_leaves_default_covers_automatic() {
    let mut conn = Connection::open_in_memory().unwrap();
    migrate(&mut conn).unwrap();
    let manual = assets::insert_asset(
        &conn,
        crate::db::assets::NewAsset {
            folder_id: None,
            file_name: "manual.mp4",
            rel_path: "manual.mp4",
            file_hash: "manual",
            file_size: 1,
            mime_type: "video/mp4",
            kind: "video",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let automatic = assets::insert_asset(
        &conn,
        crate::db::assets::NewAsset {
            folder_id: None,
            file_name: "auto.mp4",
            rel_path: "auto.mp4",
            file_hash: "auto",
            file_size: 1,
            mime_type: "video/mp4",
            kind: "video",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assets::insert_frame(&conn, manual, 1, 5000, Some("chosen.webp")).unwrap();
    conn.execute(
        "UPDATE assets SET thumb_rel='chosen.webp' WHERE id=?1",
        [manual],
    )
    .unwrap();
    conn.execute(
        "UPDATE assets SET thumb_rel='thumbnail.webp' WHERE id=?1",
        [automatic],
    )
    .unwrap();
    conn.execute_batch(include_str!("../fixtures/remove_processing_schema.sql"))
        .unwrap();
    conn.execute_batch("DROP TRIGGER schema_version_no_downgrade; ALTER TABLE assets DROP COLUMN cover_manual; UPDATE meta SET value='17' WHERE key='schema_version';").unwrap();
    // Read-only v17 source library inference used during cross-library transfer.
    assert!(assets::has_manual_cover(&conn, manual).unwrap());
    assert!(!assets::has_manual_cover(&conn, automatic).unwrap());
    migrate(&mut conn).unwrap();
    assert!(assets::has_manual_cover(&conn, manual).unwrap());
    assert!(!assets::has_manual_cover(&conn, automatic).unwrap());
    migrate(&mut conn).unwrap();
    assert!(assets::has_manual_cover(&conn, manual).unwrap());
}

#[test]
fn v16_recovers_when_columns_exist_but_schema_version_is_still_15() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta VALUES('schema_version', '15');
         CREATE TABLE assets(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           web_source_url TEXT,
           web_source_title TEXT,
           display_name TEXT
         );",
    )
    .unwrap();

    migrate(&mut conn).unwrap();

    let version: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, crate::db::SCHEMA_VERSION);

    let columns: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('assets') WHERE name IN ('web_source_url', 'web_source_title', 'display_name')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(columns, 3);
}
