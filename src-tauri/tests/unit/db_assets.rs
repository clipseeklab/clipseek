use super::*;
use crate::db::model::AssetFilter;
use rusqlite::types::Value as SqlValue;

fn insert_test_asset(conn: &Connection) -> i64 {
    conn.execute(
            "INSERT INTO assets(uid,file_name,rel_path,file_hash,file_size,mime_type,kind,imported_at,import_stage) \
             VALUES('asset-uid','physical.jpg','images/physical.jpg','asset-hash',1,'image/jpeg','image',1,'done')",
            [],
        )
        .unwrap();
    conn.last_insert_rowid()
}

#[test]
fn compiles_multi_folder_and_aspect_filters() {
    let (sql, params) = build_where(&AssetFilter {
        folder_ids: vec![3, 7],
        uncategorized: true,
        min_aspect: Some(1.25),
        max_aspect: Some(1.4),
        ..Default::default()
    });
    assert!(sql.contains("a.folder_id IN (?,?) OR a.folder_id IS NULL"));
    assert!(sql.contains("CAST(a.width AS REAL) / NULLIF(a.height, 0)"));
    assert_eq!(params.len(), 4);
}

#[test]
fn compiles_exact_asset_id_filter() {
    let (sql, params) = build_where(&AssetFilter {
        asset_id: Some(42),
        ..Default::default()
    });
    assert!(sql.contains("a.id = ?"));
    assert_eq!(params, vec![SqlValue::from(42_i64)]);
}

#[test]
fn browser_source_initializes_title_without_overwriting_user_metadata() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    let id = insert_test_asset(&conn);

    set_web_source(&conn, id, "https://example.com/post/1", "First page").unwrap();
    set_web_source(&conn, id, "https://example.com/post/2", "Second page").unwrap();
    let values: (String, String, String, String) = conn
            .query_row(
                "SELECT file_name, display_name, web_source_url, web_source_title FROM assets WHERE id=?1",
                [id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
    assert_eq!(
        values,
        (
            "physical.jpg".into(),
            "First page".into(),
            "https://example.com/post/1".into(),
            "First page".into(),
        )
    );

    set_display_name(&conn, id, Some("My title")).unwrap();
    set_web_source(&conn, id, "https://example.com/post/3", "Third page").unwrap();
    let names: (String, String) = conn
        .query_row(
            "SELECT file_name, display_name FROM assets WHERE id=?1",
            [id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(names, ("physical.jpg".into(), "My title".into()));
}

#[test]
fn empty_browser_title_does_not_create_a_display_name() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    let id = insert_test_asset(&conn);
    set_web_source(&conn, id, "https://example.com/post", "   ").unwrap();
    let display_name: Option<String> = conn
        .query_row("SELECT display_name FROM assets WHERE id=?1", [id], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(display_name, None);
}

#[test]
fn cursor_page_is_stable_and_skips_recount() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    for id in 1..=5 {
        conn.execute(
                "INSERT INTO assets(uid,file_name,rel_path,file_hash,file_size,mime_type,kind,imported_at,import_stage) \
                 VALUES(?1,?2,?3,?4,1,'image/jpeg','image',100,'done')",
                params![format!("uid-{id}"), format!("{id}.jpg"), format!("{id}.jpg"), format!("hash-{id}")],
            )
            .unwrap();
    }
    let first = query_assets(
        &conn,
        temp.path(),
        &AssetFilter {
            page: 1,
            page_size: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(first.total, 5);
    assert!(first.has_more);
    assert_eq!(
        first.items.iter().map(|asset| asset.id).collect::<Vec<_>>(),
        vec![5, 4]
    );

    let numbered_second = query_assets(
        &conn,
        temp.path(),
        &AssetFilter {
            page: 2,
            page_size: 2,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        numbered_second
            .items
            .iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>(),
        vec![3, 2],
        "MCP-style page-number pagination must not repeat the first page",
    );

    let second = query_assets(
        &conn,
        temp.path(),
        &AssetFilter {
            page: 2,
            page_size: 2,
            cursor_imported_at: Some(100),
            cursor_id: Some(4),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(second.total, 0, "Later pages must not repeat exact COUNT");
    assert!(second.has_more);
    assert_eq!(
        second
            .items
            .iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>(),
        vec![3, 2]
    );
}

#[test]
fn recovery_only_selects_live_interrupted_pipeline_stages() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    let stages = [
        "pending",
        "copied",
        "probing",
        "thumb",
        "frames",
        "reindex",
        "embedding",
        "done",
        "error:broken",
    ];
    for (index, stage) in stages.iter().enumerate() {
        conn.execute(
                "INSERT INTO assets(uid,file_name,rel_path,file_hash,file_size,mime_type,kind,imported_at,import_stage) \
                 VALUES(?1,?2,?3,?4,1,'image/jpeg','image',?5,?6)",
                params![
                    format!("recovery-uid-{index}"),
                    format!("{stage}.jpg"),
                    format!("{stage}.jpg"),
                    format!("recovery-hash-{index}"),
                    index as i64,
                    stage,
                ],
            )
            .unwrap();
    }
    conn.execute(
        "UPDATE assets SET deleted_at=1 WHERE import_stage='copied'",
        [],
    )
    .unwrap();

    let names: Vec<_> = recoverable_imports(&conn)
        .unwrap()
        .into_iter()
        .map(|(_, name)| name)
        .collect();
    assert_eq!(
        names,
        [
            "pending.jpg",
            "probing.jpg",
            "thumb.jpg",
            "frames.jpg",
            "reindex.jpg"
        ]
    );
}

#[test]
fn batch_asset_reads_preserve_order_duplicates_and_missing_id_behavior() {
    let temp = tempfile::tempdir().unwrap();
    let conn = crate::db::open_library_db(&temp.path().join("library.db")).unwrap();
    let first = insert_test_asset(&conn);
    let second = insert_asset(
        &conn,
        NewAsset {
            file_name: "second.jpg",
            rel_path: "images/second.jpg",
            file_hash: "second",
            file_size: 1,
            mime_type: "image/jpeg",
            kind: "image",
            imported_at: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let requested = vec![second, first, 999, second];
    let found = get_assets_by_ids(&conn, temp.path(), &requested).unwrap();
    assert_eq!(
        found.iter().map(|asset| asset.id).collect::<Vec<_>>(),
        vec![second, first, second]
    );
    assert!(get_assets_by_ids(&conn, temp.path(), &[])
        .unwrap()
        .is_empty());
}
