//! Model-free search contracts using real SQLite records and Qdrant vectors.

use clip_studio_lib::db::{self, model::AssetFilter, model::Paged};
use clip_studio_lib::library::{self, LibrarySession};
use clip_studio_lib::services::search::{run_search_session, SearchHit};
use clip_studio_lib::vector;
use serde_json::json;

struct Fixture {
    session: LibrarySession,
    _temp: tempfile::TempDir,
}

fn query_vector() -> Vec<f32> {
    let mut query = vec![0.0_f32; 768];
    query[0] = 1.0;
    query
}

#[test]
fn current_memory_configuration_reopens_legacy_disk_indexes_without_losing_vectors() {
    let fixture = Fixture::new();
    let asset = fixture.asset("legacy-index.jpg");
    let frame = fixture.frame(asset, 0, 1.0);
    vector::optimize_index(&fixture.session).unwrap();
    let Fixture { session, _temp } = fixture;
    let meta = session.meta.clone();
    let config_path = session
        .root
        .join(".clipstudio-open/vector/edge_config.json");
    drop(session);

    let mut config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&config_path).unwrap()).unwrap();
    let hnsw = config["vectors"]["clip"]["hnsw_config"]
        .as_object_mut()
        .unwrap();
    assert_eq!(hnsw["memory"], "cold");
    hnsw.remove("memory");
    hnsw.insert("on_disk".into(), json!(true));
    std::fs::write(&config_path, serde_json::to_vec(&config).unwrap()).unwrap();

    let reopened = library::open_session(meta).unwrap();
    let results = run_search_session(&reopened, query_vector(), None, Some(1), Some(20)).unwrap();
    assert_eq!(results.total, 1);
    assert_eq!(results.items[0].frame_id, frame);
    assert_eq!(results.items[0].asset.id, asset);
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let meta = library::init_library(temp.path(), "Search contracts").unwrap();
        Self {
            session: library::open_session(meta).unwrap(),
            _temp: temp,
        }
    }

    fn asset(&self, name: &str) -> i64 {
        let conn = self.session.db.lock().unwrap();
        let id = db::assets::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: None,
                file_name: name,
                rel_path: name,
                file_hash: name,
                file_size: 100,
                mime_type: "image/jpeg",
                kind: "image",
                imported_at: 1,
                ..Default::default()
            },
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET visual_indexed=1,import_stage='done' WHERE id=?",
            [id],
        )
        .unwrap();
        id
    }

    fn frame(&self, asset_id: i64, seq: i64, score: f32) -> i64 {
        let (id, asset) = {
            let conn = self.session.db.lock().unwrap();
            let id = db::assets::insert_frame(&conn, asset_id, seq, seq * 1000, None).unwrap();
            let asset = db::assets::get_asset(&conn, &self.session.root, asset_id)
                .unwrap()
                .unwrap();
            (id, asset)
        };
        let mut embedding = vec![0.0_f32; 768];
        embedding[0] = score;
        embedding[1] = (1.0 - score * score).sqrt();
        vector::upsert_points(
            &self.session,
            vec![(
                id,
                embedding,
                json!({
                    "asset_id": asset_id, "seq": seq, "timestamp_ms": seq * 1000,
                    "folder_id": asset.folder_id, "kind": asset.kind, "tags": asset.tags,
                    "deleted": asset.deleted_at.is_some(), "size": asset.file_size,
                    "imported_at": asset.imported_at,
                }),
            )],
        )
        .unwrap();
        id
    }

    fn search(
        &self,
        filter: Option<&AssetFilter>,
        page: Option<u32>,
        size: Option<u32>,
    ) -> Paged<SearchHit> {
        run_search_session(&self.session, query_vector(), filter, page, size).unwrap()
    }
}

#[test]
fn results_keep_multiple_frames_and_sort_equal_scores_by_frame_id() {
    let fixture = Fixture::new();
    let video = fixture.asset("video.mp4");
    fixture
        .session
        .db
        .lock()
        .unwrap()
        .execute("UPDATE assets SET kind='video' WHERE id=?", [video])
        .unwrap();
    let lower = fixture.frame(video, 1, 0.5);
    let first = fixture.frame(video, 2, 1.0);
    let second = fixture.frame(video, 3, 1.0);
    let hits = fixture.search(None, None, None);
    assert_eq!(
        hits.items
            .iter()
            .map(|hit| hit.frame_id)
            .collect::<Vec<_>>(),
        vec![first, second, lower]
    );
    assert!(hits.items.iter().all(|hit| hit.asset.id == video));
    assert_eq!(hits.items[0].seq, 2);
    assert_eq!(hits.items[0].timestamp_ms, 2000);
    assert!(!hits.items[0].frame_uid.is_empty());
}

#[test]
fn pagination_defaults_clamps_and_reports_total_after_filtering() {
    let fixture = Fixture::new();
    let asset = fixture.asset("frames.jpg");
    let ids = (0..3)
        .map(|seq| fixture.frame(asset, seq, 1.0))
        .collect::<Vec<_>>();
    let defaults = fixture.search(None, None, None);
    assert_eq!(
        (
            defaults.page,
            defaults.page_size,
            defaults.total,
            defaults.has_more
        ),
        (1, 40, 3, false)
    );
    let first = fixture.search(None, Some(0), Some(0));
    assert_eq!(
        (first.page, first.page_size, first.total, first.has_more),
        (1, 1, 3, true)
    );
    assert_eq!(first.items[0].frame_id, ids[0]);
    let second = fixture.search(None, Some(2), Some(1));
    assert_eq!(second.items[0].frame_id, ids[1]);
    assert!(second.has_more);
    let last = fixture.search(None, Some(3), Some(1));
    assert_eq!(last.items[0].frame_id, ids[2]);
    assert!(!last.has_more);
    let empty = fixture.search(None, Some(4), Some(1));
    assert!(empty.items.is_empty());
    assert_eq!(empty.total, 3);
    assert_eq!(fixture.search(None, None, Some(999)).page_size, 200);
}

#[test]
fn display_name_filtering_uses_sqlite_and_falls_back_only_for_empty_titles() {
    let fixture = Fixture::new();
    let titled = fixture.asset("original.jpg");
    let fallback = fixture.asset("blue-rock.jpg");
    fixture.frame(titled, 0, 1.0);
    fixture.frame(fallback, 0, 0.75);
    {
        let conn = fixture.session.db.lock().unwrap();
        conn.execute(
            "UPDATE assets SET display_name='海边 Blue' WHERE id=?",
            [titled],
        )
        .unwrap();
        conn.execute(
            "UPDATE assets SET display_name='   ' WHERE id=?",
            [fallback],
        )
        .unwrap();
    }
    let filter = AssetFilter {
        query: Some(" BLUE ".into()),
        ..Default::default()
    };
    let page = fixture.search(Some(&filter), None, None);
    assert_eq!(page.total, 2);
    assert_eq!(page.items[0].asset.id, titled);
    let filter = AssetFilter {
        query: Some("original".into()),
        ..Default::default()
    };
    assert_eq!(fixture.search(Some(&filter), None, None).total, 0);
    let filter = AssetFilter {
        query: Some("海边".into()),
        ..Default::default()
    };
    assert_eq!(fixture.search(Some(&filter), None, None).total, 1);
}

#[test]
fn structured_filters_exclude_other_folders_tags_and_trashed_assets() {
    let fixture = Fixture::new();
    let selected = fixture.asset("selected.jpg");
    let other = fixture.asset("other.jpg");
    let deleted = fixture.asset("deleted.jpg");
    let folder = {
        let conn = fixture.session.db.lock().unwrap();
        let folder = db::folders::insert(&conn, "Target", 1).unwrap();
        db::assets::set_asset_folder(&conn, selected, Some(folder)).unwrap();
        db::assets::set_asset_folder(&conn, deleted, Some(folder)).unwrap();
        db::tags::add_to_asset(&conn, selected, &["selected".into()]).unwrap();
        db::tags::add_to_asset(&conn, deleted, &["selected".into()]).unwrap();
        db::assets::set_deleted(&conn, &[deleted], Some(99)).unwrap();
        folder
    };
    for id in [selected, other, deleted] {
        fixture.frame(id, 0, 1.0);
    }
    let filter = AssetFilter {
        folder_id: Some(folder),
        tags: vec!["selected".into()],
        ..Default::default()
    };
    let hits = fixture.search(Some(&filter), None, None);
    assert_eq!(hits.total, 1);
    assert_eq!(hits.items[0].asset.id, selected);
}

#[test]
fn missing_sqlite_frames_and_invalid_payloads_do_not_become_results() {
    let fixture = Fixture::new();
    let asset = fixture.asset("valid.jpg");
    fixture.frame(asset, 0, 0.5);
    vector::upsert_points(
        &fixture.session,
        vec![
            (
                9000,
                query_vector(),
                json!({"asset_id": asset, "deleted": false}),
            ),
            (
                9001,
                query_vector(),
                json!({"asset_id": 0, "deleted": false}),
            ),
            (9002, query_vector(), json!({"deleted": false})),
        ],
    )
    .unwrap();
    let hits = fixture.search(None, None, None);
    assert_eq!(hits.total, 1);
    assert_eq!(hits.items[0].asset.id, asset);
}

#[test]
fn search_result_json_keeps_the_existing_camel_case_and_nested_asset_contract() {
    let fixture = Fixture::new();
    let asset = fixture.asset("result.jpg");
    fixture.frame(asset, 2, 1.0);
    let page = serde_json::to_value(fixture.search(None, None, None)).unwrap();
    assert_eq!(page["pageSize"], 40);
    assert_eq!(page["items"][0]["asset"]["fileName"], "result.jpg");
    assert!(page["items"][0].get("fileName").is_none());
    assert!(page["items"][0].get("frameUid").is_some());
    assert_eq!(page["items"][0]["timestampMs"], 2000);
}

#[test]
fn repeated_queries_preserve_result_order_pagination_and_asset_state() {
    let fixture = Fixture::new();
    let asset = fixture.asset("repeatable.jpg");
    fixture.frame(asset, 0, 1.0);
    fixture.frame(asset, 1, 1.0);
    let first = fixture.search(None, Some(1), Some(1));
    let second = fixture.search(None, Some(2), Some(1));
    assert_eq!(first.total, 2);
    assert_eq!(second.total, 2);
    assert_ne!(first.items[0].frame_uid, second.items[0].frame_uid);
    assert_eq!(first.items[0].asset.id, asset);
    assert_eq!(first.items[0].asset.deleted_at, None);
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(fixture.search(None, Some(1), Some(1))).unwrap()
    );
}
