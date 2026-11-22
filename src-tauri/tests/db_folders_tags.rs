//! Folder/tag DAO tests cover CRUD, NOCASE ordering, live counts, and foreign-key behavior.

use clip_studio_lib::db::{folders, open_library_db, tags};
use rusqlite::{params, Connection};

fn open() -> (tempfile::TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open_library_db(&dir.path().join("library.db")).unwrap();
    (dir, conn)
}

fn insert_asset(conn: &Connection, id: i64, folder_id: Option<i64>, deleted: bool) {
    conn.execute(
        "INSERT INTO assets(uid,file_name,rel_path,file_hash,file_size,mime_type,kind,imported_at,import_stage,folder_id,deleted_at) \
         VALUES(?1,?2,?2,?3,1,'image/jpeg','image',100,'done',?4,?5)",
        params![
            format!("uid-{id}"),
            format!("{id}.jpg"),
            format!("hash-{id}"),
            folder_id,
            deleted.then_some(200)
        ],
    )
    .unwrap();
}

fn tag_names(conn: &Connection) -> Vec<(String, i64)> {
    tags::list(conn)
        .unwrap()
        .into_iter()
        .map(|t| (t.name, t.asset_count))
        .collect()
}

#[test]
fn add_to_asset_trims_dedupes_and_skips_invalid_names() {
    let (_dir, conn) = open();
    insert_asset(&conn, 1, None, false);

    tags::add_to_asset(
        &conn,
        1,
        &[
            "  风景  ".to_string(),
            "风景".to_string(),
            "".to_string(),
            "   ".to_string(),
            "a,b".to_string(),
        ],
    )
    .unwrap();
    assert_eq!(
        tag_names(&conn),
        vec![("风景".into(), 1)],
        "Trim and deduplicate; ignore empty and comma-containing names"
    );

    // Repeated additions must not duplicate associations.
    tags::add_to_asset(&conn, 1, &["风景".to_string()]).unwrap();
    assert_eq!(tag_names(&conn), vec![("风景".into(), 1)]);

    // A shared tag increases the second asset's count without duplicating its tag row.
    insert_asset(&conn, 2, None, false);
    tags::add_to_asset(&conn, 2, &["风景".to_string()]).unwrap();
    assert_eq!(tag_names(&conn), vec![("风景".into(), 2)]);
}

#[test]
fn tag_list_orders_nocase_and_counts_only_live_assets() {
    let (_dir, conn) = open();
    insert_asset(&conn, 1, None, false);
    insert_asset(&conn, 2, None, true);
    tags::add_to_asset(&conn, 1, &["beta".to_string()]).unwrap();
    tags::add_to_asset(&conn, 2, &["beta".to_string(), "Alpha".to_string()]).unwrap();

    assert_eq!(
        tag_names(&conn),
        vec![("Alpha".into(), 0), ("beta".into(), 1)],
        "NOCASE ordering counts only live assets; Alpha has only trashed members and counts as zero"
    );
}

#[test]
fn tag_remove_rename_delete_round_trip() {
    let (_dir, conn) = open();
    insert_asset(&conn, 1, None, false);
    tags::add_to_asset(&conn, 1, &["x".to_string(), "y".to_string()]).unwrap();

    let stored = tags::list(&conn).unwrap();
    let x_id = stored.iter().find(|t| t.name == "x").unwrap().id;
    let y_id = stored.iter().find(|t| t.name == "y").unwrap().id;

    tags::remove_from_asset(&conn, 1, x_id).unwrap();
    assert_eq!(
        tag_names(&conn),
        vec![("x".into(), 0), ("y".into(), 1)],
        "Removal clears membership but retains the tag row"
    );

    tags::rename(&conn, y_id, "z").unwrap();
    assert!(tag_names(&conn).iter().any(|(name, _)| name == "z"));

    tags::delete(&conn, y_id).unwrap();
    let orphan_links: i64 = conn
        .query_row("SELECT COUNT(*) FROM asset_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        orphan_links, 0,
        "Deleting a tag must cascade to memberships"
    );
    assert!(tag_names(&conn).iter().all(|(name, _)| name == "x"));
}

#[test]
fn duplicate_folder_names_have_a_specific_error_and_preserve_existing_folders() {
    let (_dir, conn) = open();
    let first = folders::insert(&conn, "Posters", 100).unwrap();
    let other = folders::insert(&conn, "Sketches", 101).unwrap();
    let error = folders::insert(&conn, "Posters", 102).unwrap_err();
    assert_eq!(error.code(), "folder_duplicate");
    let error = folders::rename(&conn, other, "Posters").unwrap_err();
    assert_eq!(error.code(), "folder_duplicate");
    assert_eq!(
        folders::get_name(&conn, first).unwrap().as_deref(),
        Some("Posters")
    );
    assert_eq!(
        folders::get_name(&conn, other).unwrap().as_deref(),
        Some("Sketches")
    );
    folders::rename(&conn, first, "posters").unwrap();
    assert_eq!(
        folders::get_name(&conn, first).unwrap().as_deref(),
        Some("posters")
    );
}

#[test]
fn folder_insert_get_and_rename_round_trip() {
    let (_dir, conn) = open();
    let id = folders::insert(&conn, "Trips", 100).unwrap();

    // Case-insensitive lookup must match the uniqueness constraint.
    let found = folders::get_by_name(&conn, "trips")
        .unwrap()
        .expect("NOCASE match");
    assert_eq!(found.id, id);
    assert_eq!(found.name, "Trips");
    assert_eq!(found.asset_count, 0);
    assert!(folders::get_by_name(&conn, "missing").unwrap().is_none());

    assert_eq!(
        folders::get_name(&conn, id).unwrap().as_deref(),
        Some("Trips")
    );
    assert!(folders::get_name(&conn, 999).unwrap().is_none());

    folders::rename(&conn, id, "Journeys").unwrap();
    assert_eq!(
        folders::get_name(&conn, id).unwrap().as_deref(),
        Some("Journeys")
    );
}

#[test]
fn folder_list_orders_nocase_and_counts_only_live_assets() {
    let (_dir, conn) = open();
    let b = folders::insert(&conn, "beta", 100).unwrap();
    folders::insert(&conn, "Alpha", 101).unwrap();
    insert_asset(&conn, 1, Some(b), false);
    insert_asset(&conn, 2, Some(b), true);

    let stored = folders::list(&conn).unwrap();
    let names: Vec<&str> = stored.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, vec!["Alpha", "beta"], "NOCASE order");
    assert_eq!(
        stored[1].asset_count, 1,
        "Trashed assets do not contribute to asset_count"
    );
    assert_eq!(stored[0].asset_count, 0);
}

#[test]
fn folder_delete_nulls_folder_id_and_count_excludes_deleted() {
    let (_dir, conn) = open();
    let id = folders::insert(&conn, "soon-gone", 100).unwrap();
    insert_asset(&conn, 1, Some(id), false);
    insert_asset(&conn, 2, Some(id), true);
    assert_eq!(folders::count_assets(&conn, id).unwrap(), 1);

    folders::delete(&conn, id).unwrap();
    assert!(folders::get_by_name(&conn, "soon-gone").unwrap().is_none());
    let orphaned: Option<i64> = conn
        .query_row(
            "SELECT folder_id FROM assets WHERE uid = 'uid-1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        orphaned, None,
        "Deleting a folder makes assets uncategorized"
    );
}

#[test]
fn folder_pages_are_bounded_searchable_and_count_only_live_assets() {
    let (_dir, conn) = open();
    for i in 0..85 {
        folders::insert(&conn, &format!("Folder {i:03}"), 100).unwrap();
    }
    let first = folders::page(&conn, "", 1).unwrap();
    assert_eq!(
        (first.items.len(), first.total, first.has_more),
        (40, 85, true)
    );
    let second = folders::page(&conn, "", 2).unwrap();
    let last = folders::page(&conn, "", 3).unwrap();
    let ids = first
        .items
        .iter()
        .chain(&second.items)
        .chain(&last.items)
        .map(|f| f.id)
        .collect::<std::collections::HashSet<_>>();
    assert_eq!(ids.len(), 85);
    assert_eq!(last.items.len(), 5);
    assert!(!last.has_more);
    assert_eq!(first.items[0].name, "Folder 084");
    let id = folders::insert(&conn, "Literal %_ Folder", 100).unwrap();
    insert_asset(&conn, 1, Some(id), false);
    insert_asset(&conn, 2, Some(id), true);
    let found = folders::page(&conn, "  %_  ", 1).unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.items[0].asset_count, 1);
    assert_eq!(folders::page(&conn, "FOLDER 084", 1).unwrap().total, 1);
    assert!(folders::page(&conn, "", 0).is_err());
    assert!(folders::page(&conn, "", i64::MAX).is_err());
    assert!(folders::get(&conn, 99999).unwrap().is_none());
}

#[test]
fn sidebar_pins_are_bounded_ordered_persisted_and_library_local() {
    let (dir, conn) = open();
    for i in (0..35).rev() {
        folders::insert(&conn, &format!("Folder {i:03}"), 100).unwrap();
    }
    let initial = folders::sidebar(&conn).unwrap();
    assert_eq!(initial.total, 35);
    assert_eq!(initial.items.len(), 10);
    assert_eq!(initial.display_limit, 10);
    assert_eq!(initial.items[0].name, "Folder 000");
    let all = folders::list(&conn).unwrap();
    assert!(folders::set_sidebar(&conn, &all.iter().map(|f| f.id).collect::<Vec<_>>()).is_err());
    assert_eq!(folders::sidebar(&conn).unwrap().items.len(), 10);
    assert!(folders::set_sidebar(&conn, &[all[0].id, all[0].id]).is_err());
    assert!(folders::set_sidebar(&conn, &[99999]).is_err());
    let ids = vec![all[24].id, all[2].id, all[0].id];
    folders::set_sidebar(&conn, &ids).unwrap();
    let new = folders::insert(&conn, "AAA new", 100).unwrap();
    assert_eq!(
        folders::sidebar(&conn)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        ids
    );
    folders::rename(&conn, ids[0], "Renamed").unwrap();
    assert_eq!(folders::sidebar(&conn).unwrap().items[0].name, "Renamed");
    folders::delete(&conn, ids[1]).unwrap();
    drop(conn);
    let conn = open_library_db(&dir.path().join("library.db")).unwrap();
    assert_eq!(
        folders::sidebar(&conn)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        vec![ids[0], ids[2]]
    );
    folders::pin(&conn, new, true).unwrap();
    folders::pin(&conn, ids[0], false).unwrap();
    assert_eq!(
        folders::sidebar(&conn)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        vec![new, ids[2]]
    );
    let (_other_dir, other) = open();
    assert!(folders::sidebar(&other).unwrap().items.is_empty());
    folders::set_sidebar(&conn, &[]).unwrap();
    assert!(folders::sidebar(&conn).unwrap().items.is_empty());
}

#[test]
fn pin_promotes_new_folders_and_evicts_only_the_last_shortcut_at_capacity() {
    let (_dir, conn) = open();
    let ids = (0..32)
        .map(|i| folders::insert(&conn, &format!("Folder {i:02}"), 100).unwrap())
        .collect::<Vec<_>>();
    folders::set_sidebar(&conn, &ids[..30]).unwrap();
    insert_asset(&conn, 1, Some(ids[29]), false);
    let result = folders::pin(&conn, ids[30], true).unwrap();
    let expected = std::iter::once(ids[30])
        .chain(ids[..29].iter().copied())
        .collect::<Vec<_>>();
    assert_eq!(
        result.items.iter().map(|f| f.id).collect::<Vec<_>>(),
        expected
    );
    assert_eq!(result.total, 32);
    assert!(folders::get(&conn, ids[29]).unwrap().is_some());
    assert_eq!(folders::count_assets(&conn, ids[29]).unwrap(), 1);
    assert!(folders::pin(&conn, 999999, true).is_err());
    assert_eq!(
        folders::sidebar(&conn)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn pin_promotes_to_front_without_duplicates_and_unpin_preserves_remaining_order() {
    let (_dir, conn) = open();
    let ids = (0..3)
        .map(|i| folders::insert(&conn, &format!("Folder {i}"), 100).unwrap())
        .collect::<Vec<_>>();
    folders::set_sidebar(&conn, &ids[..2]).unwrap();
    assert_eq!(
        folders::pin(&conn, ids[2], true)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[0], ids[1]]
    );
    assert_eq!(
        folders::pin(&conn, ids[0], true)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        vec![ids[0], ids[2], ids[1]]
    );
    assert_eq!(
        folders::pin(&conn, ids[0], false)
            .unwrap()
            .items
            .iter()
            .map(|f| f.id)
            .collect::<Vec<_>>(),
        vec![ids[2], ids[1]]
    );
}

#[test]
fn sidebar_display_limit_is_validated_persisted_and_preserves_hidden_pins() {
    let (dir, conn) = open();
    let ids = (0..30)
        .map(|i| folders::insert(&conn, &format!("Folder {i}"), 100).unwrap())
        .collect::<Vec<_>>();
    let result = folders::set_sidebar_with_limit(&conn, &ids, Some(5)).unwrap();
    assert_eq!(result.display_limit, 5);
    assert_eq!(result.items.len(), 30);
    for limit in [4, 31, -1] {
        assert!(folders::set_sidebar_with_limit(&conn, &[], Some(limit)).is_err());
    }
    assert_eq!(folders::sidebar(&conn).unwrap().items.len(), 30);
    assert_eq!(folders::sidebar(&conn).unwrap().display_limit, 5);
    drop(conn);
    let conn = open_library_db(&dir.path().join("library.db")).unwrap();
    assert_eq!(folders::sidebar(&conn).unwrap().display_limit, 5);
    assert_eq!(
        folders::set_sidebar_with_limit(&conn, &ids, Some(30))
            .unwrap()
            .display_limit,
        30
    );
    folders::set_sidebar(&conn, &ids[..10]).unwrap();
    assert_eq!(folders::sidebar(&conn).unwrap().display_limit, 30);
    let (_other_dir, other) = open();
    assert_eq!(folders::sidebar(&other).unwrap().display_limit, 10);
}

#[test]
fn folder_sorting_orders_before_pagination_and_breaks_timestamp_ties_stably() {
    let (_dir, conn) = open();
    let mut records = Vec::new();
    for i in 0..85 {
        let name = format!("Folder {:03}", 84 - i);
        let time = (i * 17) % 13;
        let id = folders::insert(&conn, &name, time).unwrap();
        records.push((id, name, time));
    }
    for sort in ["name_asc", "name_desc", "created_asc", "created_desc"] {
        let mut expected = records.clone();
        expected.sort_by(|a, b| match sort {
            "name_asc" => a.1.cmp(&b.1).then(a.0.cmp(&b.0)),
            "name_desc" => b.1.cmp(&a.1).then(b.0.cmp(&a.0)),
            "created_asc" => a.2.cmp(&b.2).then(a.0.cmp(&b.0)),
            _ => b.2.cmp(&a.2).then(b.0.cmp(&a.0)),
        });
        let mut actual = Vec::new();
        for page in 1..=3 {
            let result = folders::page_sorted(&conn, "Folder", page, sort).unwrap();
            assert_eq!(result.total, 85);
            assert_eq!(result.has_more, page < 3);
            actual.extend(result.items.into_iter().map(|f| f.id));
        }
        assert_eq!(
            actual,
            expected.iter().map(|f| f.0).collect::<Vec<_>>(),
            "{sort}"
        );
        let filtered = folders::page_sorted(&conn, "08", 1, sort).unwrap();
        assert_eq!(
            filtered.items.iter().map(|f| f.id).collect::<Vec<_>>(),
            expected
                .iter()
                .filter(|f| f.1.contains("08"))
                .map(|f| f.0)
                .collect::<Vec<_>>()
        );
    }
    assert!(folders::page_sorted(&conn, "", 1, "name; DROP TABLE folders").is_err());
    let a = folders::insert(&conn, "alpha", 100).unwrap();
    let b = folders::insert(&conn, "Beta", 100).unwrap();
    let found = folders::page_sorted(&conn, "", 1, "name_asc").unwrap();
    assert_eq!(found.items[0].id, a);
    assert_eq!(found.items[1].id, b);
}
