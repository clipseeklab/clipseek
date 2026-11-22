use super::*;

#[test]
fn multi_value_groups_are_or_within_and_between_groups() {
    let filter = build_filter(&db::model::AssetFilter {
        kinds: vec!["image".into(), "video".into()],
        colors: vec!["red".into(), "blue".into()],
        ..Default::default()
    })
    .unwrap();
    let nested: Vec<_> = filter
        .must
        .unwrap()
        .into_iter()
        .filter_map(|condition| {
            if let Condition::Filter(inner) = condition {
                Some(inner)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        nested.len(),
        2,
        "Kind and color groups must be independent AND conditions"
    );
    assert!(nested
        .iter()
        .all(|group| group.should.as_ref().is_some_and(|v| v.len() == 2)));
}

#[test]
fn asset_id_is_a_qdrant_prefilter() {
    let filter = build_filter(&db::model::AssetFilter {
        asset_id: Some(42),
        ..Default::default()
    })
    .unwrap();
    let debug = format!("{filter:?}");
    assert!(debug.contains("asset_id"));
    assert!(debug.contains("42"));
}

#[test]
fn final_optimize_completes_without_redundant_flush_and_keeps_points_searchable() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "优化测试库").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    let vector = vec![1.0_f32; EMBED_DIM];
    upsert_points(
        &session,
        vec![(1, vector.clone(), serde_json::json!({ "asset_id": 1 }))],
    )
    .unwrap();

    optimize_index(&session).unwrap();

    let hits = search(&session, vector, None, 1).unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, PointId::NumId(1));
}
