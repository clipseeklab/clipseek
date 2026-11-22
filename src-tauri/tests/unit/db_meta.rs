use super::*;

#[test]
fn v10_rows_receive_unique_uuid_v7_ids_and_constraints() {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO meta VALUES('schema_version', '10');
         CREATE TABLE assets(
           id INTEGER PRIMARY KEY AUTOINCREMENT,
           folder_id INTEGER,
           imported_at INTEGER NOT NULL DEFAULT 0,
           deleted_at INTEGER
         );
         CREATE TABLE frames(id INTEGER PRIMARY KEY AUTOINCREMENT);
         INSERT INTO assets DEFAULT VALUES;
         INSERT INTO assets DEFAULT VALUES;
         INSERT INTO frames DEFAULT VALUES;",
    )
    .unwrap();

    migrate(&mut conn).unwrap();
    let asset_uids: Vec<String> = conn
        .prepare("SELECT uid FROM assets ORDER BY id")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(asset_uids.len(), 2);
    assert_ne!(asset_uids[0], asset_uids[1]);
    assert!(asset_uids
        .iter()
        .all(|uid| uuid::Uuid::parse_str(uid).is_ok_and(|parsed| parsed.get_version_num() == 7)));
    assert!(conn
        .execute("UPDATE assets SET uid='' WHERE id=1", [])
        .is_err());
    assert!(conn
        .execute("INSERT INTO assets DEFAULT VALUES", [])
        .is_err());
    let version: i64 = conn
        .query_row(
            "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(version, crate::db::SCHEMA_VERSION);
    let source_columns: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('assets') WHERE name IN ('web_source_url', 'web_source_title', 'display_name')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(source_columns, 3);
}
