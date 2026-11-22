use super::*;

fn policy(enabled: bool) -> db::indexing::IndexPolicy {
    db::indexing::IndexPolicy {
        visual: enabled,
        subtitles: enabled,
    }
}

#[test]
fn resumed_job_keeps_original_tags_and_policy_when_page_and_settings_change() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Tag recovery").unwrap();
    let paths = vec!["/source/photo.jpg".to_owned()];
    {
        let session = crate::library::open_session(meta.clone()).unwrap();
        save(
            &session,
            Job {
                id: "tag-job",
                mode: "paths",
                paths: &paths,
                folder: None,
                policy: policy(false),
                tags: Some(&["旅行".into(), "收藏".into()]),
            },
        )
        .unwrap();
    }
    let session = crate::library::open_session(meta).unwrap();
    let intent = resolve(
        &session,
        Some("tag-job"),
        policy(true),
        Some(vec!["新页面标签".into()]),
    )
    .unwrap();
    assert!(!intent.policy.visual);
    assert!(!intent.policy.subtitles);
    assert_eq!(
        intent.tags.as_ref().unwrap(),
        &vec!["旅行".to_string(), "收藏".to_string()]
    );
    save(
        &session,
        Job {
            id: "tag-job",
            mode: "paths",
            paths: &paths,
            folder: None,
            policy: intent.policy,
            tags: intent.tags.as_deref(),
        },
    )
    .unwrap();
    assert_eq!(
        db::import_jobs::get(&session.read_db(), "tag-job")
            .unwrap()
            .unwrap()
            .tags,
        intent.tags.unwrap()
    );
}

#[test]
fn tag_intent_failure_rolls_back_the_entire_new_job() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Atomic tag intent").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    session.db.lock().unwrap().execute_batch("CREATE TRIGGER reject_tag_context BEFORE INSERT ON meta WHEN NEW.key LIKE 'import_job_tags.v1.%' BEGIN SELECT RAISE(ABORT, 'injected tag write failure'); END;").unwrap();
    let result = save(
        &session,
        Job {
            id: "failed-job",
            mode: "paths",
            paths: &["/source/a.jpg".into()],
            folder: None,
            policy: policy(false),
            tags: Some(&["旅行".into()]),
        },
    );
    assert!(result.is_err());
    assert!(db::import_jobs::get(&session.read_db(), "failed-job")
        .unwrap()
        .is_none());
}

#[test]
fn recovered_tags_apply_to_new_and_duplicate_assets_without_removing_existing_tags() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Tag registration").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    let ids = {
        let conn = session.db.lock().unwrap();
        let mut ids = Vec::new();
        for name in ["new", "duplicate"] {
            let id = db::assets::insert_asset(
                &conn,
                crate::db::assets::NewAsset {
                    folder_id: None,
                    file_name: name,
                    rel_path: name,
                    file_hash: name,
                    file_size: 1,
                    mime_type: "image/png",
                    kind: "image",
                    imported_at: 1,
                    uid: Some(name),
                    ..Default::default()
                },
            )
            .unwrap();
            ids.push(id);
        }
        db::tags::add_to_asset(&conn, ids[1], &["已有标签".into()]).unwrap();
        ids
    };
    let batch = crate::import::RegisteredBatch {
        pending: vec![(ids[0], "new".into())],
        duplicates: vec![ids[1]],
        summary: Default::default(),
        aborted: false,
    };
    apply_tags(&session, &batch, Some(&["旅行".into(), "收藏".into()]));
    for id in ids {
        let asset = db::assets::get_asset(&session.read_db(), &session.root, id)
            .unwrap()
            .unwrap();
        assert!(asset.tags.contains(&"旅行".into()));
        assert!(asset.tags.contains(&"收藏".into()));
        if id == batch.duplicates[0] {
            assert!(asset.tags.contains(&"已有标签".into()));
        }
    }
}

#[test]
fn a_failed_path_admission_leaves_no_partial_handoff_or_tag_evidence() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Atomic handoff").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    session.db.lock().unwrap().execute_batch("CREATE TRIGGER reject_processing BEFORE INSERT ON processing_tasks BEGIN SELECT RAISE(ABORT, 'injected queue failure'); END;").unwrap();
    let paths = vec!["/source/photo.jpg".to_owned()];
    assert!(save_paths(
        &session,
        Job {
            id: "not-accepted",
            mode: "paths",
            paths: &paths,
            folder: None,
            policy: policy(false),
            tags: Some(&["Travel".into()]),
        },
        &[std::path::PathBuf::from(&paths[0])]
    )
    .is_err());
    assert!(!db::import_jobs::has_submission(&session.read_db(), "not-accepted").unwrap());
    let tags: i64 = session
        .read_db()
        .query_row(
            "SELECT COUNT(*) FROM meta WHERE key='import_job_tags.v1.not-accepted'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(tags, 0);
}
