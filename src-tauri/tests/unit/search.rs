use super::*;

#[test]
fn search_hit_serializes_asset_as_nested_object() {
    let hit = SearchHit {
        score: 0.75,
        frame_id: 1,
        frame_uid: "018f0000-0000-7000-8000-000000000001".into(),
        seq: 0,
        timestamp_ms: 0,
        frame_path: None,
        asset: Asset {
            id: 7,
            uid: "018f0000-0000-7000-8000-000000000007".into(),
            folder_id: None,
            folder_name: None,
            file_name: "coat.jpg".into(),
            display_name: None,
            rel_path: "images/70-01/1/coat.jpg".into(),
            abs_path: "/tmp/coat.jpg".into(),
            file_size: 100,
            mime_type: "image/jpeg".into(),
            kind: "image".into(),
            width: Some(640),
            height: Some(480),
            duration_ms: None,
            color: None,
            imported_at: 1,
            deleted_at: None,
            processing_tasks: Vec::new(),
            visual_indexed: true,
            subtitle_indexed: false,
            subtitle_available: false,
            import_stage: "done".into(),
            storage: "copy",
            source_path: None,
            source_mtime: None,
            source_missing: false,
            web_source_url: None,
            web_source_title: None,
            tags: vec![],
            thumb_path: None,
        },
    };
    let json = serde_json::to_value(hit).unwrap();
    assert_eq!(json["asset"]["fileName"], "coat.jpg");
    assert!(json.get("fileName").is_none());
}

#[test]
fn filename_filter_prefers_display_name_and_falls_back_to_file_name() {
    let mut asset = search_hit_asset();
    assert_eq!(searchable_asset_name(&asset), "coat.jpg");
    asset.display_name = Some("冬季外套".into());
    assert_eq!(searchable_asset_name(&asset), "冬季外套");
}

fn search_hit_asset() -> Asset {
    Asset {
        id: 7,
        uid: "018f0000-0000-7000-8000-000000000007".into(),
        folder_id: None,
        folder_name: None,
        file_name: "coat.jpg".into(),
        display_name: None,
        rel_path: "images/70-01/1/coat.jpg".into(),
        abs_path: "/tmp/coat.jpg".into(),
        file_size: 100,
        mime_type: "image/jpeg".into(),
        kind: "image".into(),
        width: Some(640),
        height: Some(480),
        duration_ms: None,
        color: None,
        imported_at: 1,
        deleted_at: None,
        processing_tasks: Vec::new(),
        visual_indexed: true,
        subtitle_indexed: false,
        subtitle_available: false,
        import_stage: "done".into(),
        storage: "copy",
        source_path: None,
        source_mtime: None,
        source_missing: false,
        web_source_url: None,
        web_source_title: None,
        tags: vec![],
        thumb_path: None,
    }
}
