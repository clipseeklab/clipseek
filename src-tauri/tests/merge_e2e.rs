//! End-to-end merge: import into two libraries, merge, verify destination content and source
//! removal.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use clip_studio_lib::config::GlobalConfig;
use clip_studio_lib::db;
use clip_studio_lib::{import, library, transfer};

/// Generate standalone SVG fixtures so missing external sample data cannot silently skip the
/// test.
fn test_images(dir: &Path, n: usize) -> Vec<PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    (0..n)
        .map(|i| {
            let path = dir.join(format!("material-{i}.svg"));
            let color = format!("#{:06x}", (0x224466u32 + i as u32 * 0x171717) & 0xffffff);
            std::fs::write(
                &path,
                format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48"><rect width="64" height="48" fill="{color}"/><text x="4" y="24">{i}</text></svg>"#),
            )
            .unwrap();
            path
        })
        .collect()
}

#[test]
fn merge_library_e2e() {
    let temp = tempfile::Builder::new()
        .prefix("clip-studio-merge-")
        .tempdir()
        .unwrap();
    let base = temp.path();
    let images = test_images(&base.join("inputs"), 6);

    // The source contains six images plus folder and tag metadata.
    let src_meta = library::init_library(&base.join("src"), "源库").unwrap();
    let src = Arc::new(library::open_session(src_meta.clone()).unwrap());
    let cancel = AtomicBool::new(false);
    let s1 = import::run_import(
        &src,
        &images[..3],
        None,
        &cancel,
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(s1.done, 3);

    let fid = clip_studio_lib::services::folders::create_folder(&src, "设计")
        .unwrap()
        .id;
    let s2 = import::run_import(
        &src,
        &images[3..],
        Some(fid),
        &cancel,
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(s2.done, 3);
    {
        let conn = src.db.lock().unwrap();
        let first_id = conn
            .query_row("SELECT id FROM assets LIMIT 1", [], |r| r.get::<_, i64>(0))
            .unwrap();
        db::tags::add_to_asset(&conn, first_id, &["风景".into(), "自然".into()]).unwrap();
    }
    let source_uids: std::collections::HashSet<String> = {
        let conn = src.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT uid FROM assets").unwrap();
        stmt.query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap()
    };

    // Preimport one matching destination asset to verify content-hash deduplication.
    let dst_meta = library::init_library(&base.join("dst"), "目标库").unwrap();
    let dst = Arc::new(library::open_session(dst_meta.clone()).unwrap());
    let s3 = import::run_import(
        &dst,
        &images[..1],
        None,
        &cancel,
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(s3.done, 1);

    // Merge using an in-memory configuration that never writes user settings.
    let mut cfg = GlobalConfig::load_from(base.join("global-config.json")).unwrap();
    cfg.libraries.push(src_meta.clone());
    cfg.libraries.push(dst_meta.clone());
    // Release the source WAL handle and reuse the open destination.
    drop(src);
    let mut open_sessions = std::collections::HashMap::new();
    open_sessions.insert(dst_meta.id.clone(), dst.clone());
    let progress_events = std::sync::Mutex::new(Vec::new());
    let merge_result = transfer::merge_library_with_progress(
        &mut cfg,
        &src_meta.id,
        &dst_meta.id,
        &open_sessions,
        &|event| {
            progress_events.lock().unwrap().push(event.clone());
        },
    );
    if let Err(error) = &merge_result {
        eprintln!(
            "Progress before merge failure: {:?}",
            progress_events.lock().unwrap()
        );
        panic!("Merge failed: {error:?}");
    }
    let summary = merge_result.unwrap();
    // One of six source assets duplicates destination content and is skipped.
    assert_eq!(summary.assets, 5, "{summary:?}");
    assert_eq!(summary.failed, 0);
    assert_eq!(summary.folders, 1);
    assert_eq!(summary.skipped, 1);

    // Verify the destination.
    {
        let conn = dst.db.lock().unwrap();
        let filter = db::model::AssetFilter {
            page: 1,
            page_size: 100,
            ..Default::default()
        };
        let paged = db::assets::query_assets(&conn, &dst.root, &filter).unwrap();
        assert_eq!(
            paged.total, 6,
            "Destination count: one existing plus five merged"
        );
        // Verify folder mapping.
        let folders = db::folders::list(&conn).unwrap();
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].name, "设计");
        assert_eq!(folders[0].asset_count, 3, "Design folder asset count");
        // Verify tag transfer.
        let tags = db::tags::list(&conn).unwrap();
        assert_eq!(tags.len(), 2, "Tag count");
        // Verify managed files exist.
        for a in &paged.items {
            assert!(
                std::path::Path::new(&a.abs_path).exists(),
                "Missing file: {}",
                a.abs_path
            );
        }
        let retained = paged
            .items
            .iter()
            .filter(|a| source_uids.contains(&a.uid))
            .count();
        assert_eq!(retained, 5, "Nonduplicate global UIDs must survive merging");
        let unique: std::collections::HashSet<_> = paged.items.iter().map(|a| &a.uid).collect();
        assert_eq!(
            unique.len(),
            paged.items.len(),
            "Destination must not contain duplicate UIDs"
        );
    }
    let events = progress_events.lock().unwrap();
    assert_eq!(events.first().unwrap().phase, "checking_space");
    assert_eq!(events.last().unwrap().status, "completed");
    // Verify removal of the source directory.
    assert!(
        !base.join("src").exists(),
        "Source library directory must be removed"
    );
    // Verify removal of the source registration.
    assert_eq!(cfg.libraries.len(), 1);
}

#[test]
fn interrupted_asset_copy_is_cleaned_and_retried_by_uid() {
    let temp = tempfile::tempdir().unwrap();
    let images = test_images(&temp.path().join("inputs"), 1);
    let src_meta = library::init_library(&temp.path().join("src"), "src").unwrap();
    let dst_meta = library::init_library(&temp.path().join("dst"), "dst").unwrap();
    let src = Arc::new(library::open_session(src_meta).unwrap());
    let dst = Arc::new(library::open_session(dst_meta).unwrap());
    let cancel = AtomicBool::new(false);
    assert_eq!(
        import::run_import(
            &src,
            &images,
            None,
            &cancel,
            import::ImportMode::Reference,
            &|_, _, _| {},
            &|_| {}
        )
        .done,
        1
    );
    let source = {
        let conn = src.db.lock().unwrap();
        db::assets::query_assets(&conn, &src.root, &Default::default())
            .unwrap()
            .items
            .remove(0)
    };
    {
        let conn = dst.db.lock().unwrap();
        db::assets::insert_asset(
            &conn,
            clip_studio_lib::db::assets::NewAsset {
                folder_id: None,
                file_name: &source.file_name,
                rel_path: "images/missing",
                file_hash: &clip_studio_lib::import::scanner::file_hash(std::path::Path::new(
                    &source.abs_path,
                ))
                .unwrap(),
                file_size: source.file_size,
                mime_type: &source.mime_type,
                kind: &source.kind,
                imported_at: source.imported_at,
                uid: Some(&source.uid),
                ..Default::default()
            },
        )
        .unwrap();
    }
    let copied = transfer::copy_asset_to_session(&src, source.id, &dst, None).unwrap();
    let conn = dst.db.lock().unwrap();
    let repaired = db::assets::get_asset(&conn, &dst.root, copied)
        .unwrap()
        .unwrap();
    assert_eq!(repaired.uid, source.uid);
    assert!(std::path::Path::new(&repaired.abs_path).is_file());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn move_assets_retries_completed_target_and_only_then_deletes_source() {
    let temp = tempfile::tempdir().unwrap();
    let images = test_images(&temp.path().join("inputs"), 2);
    let src_meta = library::init_library(&temp.path().join("src"), "src").unwrap();
    let dst_meta = library::init_library(&temp.path().join("dst"), "dst").unwrap();
    let src = Arc::new(library::open_session(src_meta.clone()).unwrap());
    let dst = Arc::new(library::open_session(dst_meta.clone()).unwrap());
    let cancel = AtomicBool::new(false);
    assert_eq!(
        import::run_import(
            &src,
            &images,
            None,
            &cancel,
            import::ImportMode::Reference,
            &|_, _, _| {},
            &|_| {}
        )
        .done,
        2
    );
    let ids: Vec<i64> = {
        let conn = src.db.lock().unwrap();
        let mut statement = conn.prepare("SELECT id FROM assets ORDER BY id").unwrap();
        let rows = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        rows
    };

    // Simulate interruption after destination commit but before source cleanup.
    transfer::copy_asset_to_session(&src, ids[0], &dst, None).unwrap();
    drop(dst);
    let mut config = GlobalConfig::load_from(temp.path().join("global-config.json")).unwrap();
    config
        .libraries
        .extend([src_meta.clone(), dst_meta.clone()]);
    config.active_library_id = Some(src_meta.id.clone());
    let state = clip_studio_lib::AppState {
        mcp: Default::default(),
        config: std::sync::RwLock::new(config),
        session: std::sync::RwLock::new(Some(src.clone())),
        session_cache: std::sync::Mutex::new(std::collections::HashMap::new()),
        session_switch_lock: std::sync::Mutex::new(()),
        import_ctl: import::ImportControl::default(),
        clip: clip_studio_lib::clip::ClipHandle::default(),
        operation_running: AtomicBool::new(false),
        source_check_running: Arc::new(AtomicBool::new(false)),
        source_check_cancel: Arc::new(AtomicBool::new(false)),
    };
    let events = std::sync::Mutex::new(Vec::new());
    let moved =
        transfer::move_assets_to_library_with_progress(&state, &ids, &dst_meta.id, &|event| {
            events.lock().unwrap().push(event.clone())
        })
        .unwrap();
    assert_eq!(moved, 2);
    assert_eq!(
        src.db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );

    let reopened = library::open_session(dst_meta).unwrap();
    assert_eq!(
        reopened
            .db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let events = events.lock().unwrap();
    assert_eq!(events.last().unwrap().status, "completed");
    assert_eq!(events.last().unwrap().skipped, 1);
}

#[test]
fn merge_rejects_nonempty_source_trash_before_writing_target() {
    let temp = tempfile::tempdir().unwrap();
    let images = test_images(&temp.path().join("inputs"), 1);
    let src_meta = library::init_library(&temp.path().join("src"), "src").unwrap();
    let dst_meta = library::init_library(&temp.path().join("dst"), "dst").unwrap();
    let src = Arc::new(library::open_session(src_meta.clone()).unwrap());
    let cancel = AtomicBool::new(false);
    assert_eq!(
        import::run_import(
            &src,
            &images,
            None,
            &cancel,
            import::ImportMode::Reference,
            &|_, _, _| {},
            &|_| {}
        )
        .done,
        1
    );
    let asset_id = src
        .db
        .lock()
        .unwrap()
        .query_row("SELECT id FROM assets", [], |row| row.get(0))
        .unwrap();
    clip_studio_lib::services::assets::trash_assets(&src, &[asset_id]).unwrap();
    drop(src);

    let mut config = GlobalConfig::load_from(temp.path().join("global-config.json")).unwrap();
    config
        .libraries
        .extend([src_meta.clone(), dst_meta.clone()]);
    let error =
        transfer::merge_library(&mut config, &src_meta.id, &dst_meta.id, &Default::default())
            .unwrap_err();
    assert!(error.to_string().contains("trashed"));
    assert!(temp.path().join("src").is_dir());
    assert_eq!(config.libraries.len(), 2);
    let dst = library::open_session(dst_meta).unwrap();
    assert_eq!(
        dst.db
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

/// Reference merges preserve storage mode and source paths without copying or moving the
/// external source.
#[test]
fn merge_preserves_reference_assets() {
    let temp = tempfile::Builder::new()
        .prefix("clip-studio-merge-ref-")
        .tempdir()
        .unwrap();
    let base = temp.path();
    let images = test_images(&base.join("inputs"), 1);
    let source = &images[0];

    // Import source references without copying original files.
    let src_meta = library::init_library(&base.join("src"), "引用源库").unwrap();
    let src = Arc::new(library::open_session(src_meta.clone()).unwrap());
    let cancel = AtomicBool::new(false);
    let summary = import::run_import(
        &src,
        std::slice::from_ref(source),
        None,
        &cancel,
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(summary.done, 1);
    // The missing-source flag belongs to the reference and must survive merging.
    {
        let conn = src.db.lock().unwrap();
        let id: i64 = conn
            .query_row("SELECT id FROM assets LIMIT 1", [], |row| row.get(0))
            .unwrap();
        db::assets::set_source_missing(&conn, id, true).unwrap();
    }

    let dst_meta = library::init_library(&base.join("dst"), "引用目标库").unwrap();
    let dst = Arc::new(library::open_session(dst_meta.clone()).unwrap());
    drop(src);

    let mut cfg = GlobalConfig::load_from(base.join("global-config.json")).unwrap();
    cfg.libraries.push(src_meta.clone());
    cfg.libraries.push(dst_meta.clone());
    let mut open_sessions = std::collections::HashMap::new();
    open_sessions.insert(dst_meta.id.clone(), dst.clone());
    let summary = transfer::merge_library_with_progress(
        &mut cfg,
        &src_meta.id,
        &dst_meta.id,
        &open_sessions,
        &|_| {},
    )
    .unwrap();
    assert_eq!(summary.assets, 1, "{summary:?}");
    assert_eq!(summary.failed, 0);

    // Preserve the same reference source path and leave its file untouched.
    let conn = dst.db.lock().unwrap();
    let filter = db::model::AssetFilter {
        page: 1,
        page_size: 10,
        ..Default::default()
    };
    let paged = db::assets::query_assets(&conn, &dst.root, &filter).unwrap();
    drop(conn);
    assert_eq!(paged.total, 1);
    let asset = &paged.items[0];
    assert_eq!(asset.storage, "reference");
    assert_eq!(
        asset.source_path.as_deref(),
        Some(source.to_string_lossy().as_ref())
    );
    assert_eq!(asset.abs_path, source.to_string_lossy());
    assert!(
        asset.source_missing,
        "Missing-source flags must survive reference transfer"
    );
    assert!(source.exists(), "Source files must stay in place");
    // Derived thumbnails are copied to the destination during merging.
    assert!(asset
        .thumb_path
        .as_ref()
        .is_some_and(|p| std::path::Path::new(p).exists()));
    // Removing the source library must not affect external reference files.
    assert!(!src_meta.path.is_empty() && !std::path::Path::new(&src_meta.path).exists());
}

/// Mixed legacy libraries preserve managed and reference storage independently during
/// merging.
#[test]
fn merge_mixed_storage_library() {
    let temp = tempfile::Builder::new()
        .prefix("clip-studio-merge-mix-")
        .tempdir()
        .unwrap();
    let base = temp.path();
    let images = test_images(&base.join("inputs"), 2);

    let src_meta = library::init_library(&base.join("src"), "混合源库").unwrap();
    let src = Arc::new(library::open_session(src_meta.clone()).unwrap());
    let cancel = AtomicBool::new(false);
    let copied = import::run_import(
        &src,
        &images[..1],
        None,
        &cancel,
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(copied.done, 1);
    {
        let conn = src.db.lock().unwrap();
        let (id, rel): (i64, String) = conn
            .query_row("SELECT id,rel_path FROM assets LIMIT 1", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .unwrap();
        let owned = src.root.join(rel);
        std::fs::create_dir_all(owned.parent().unwrap()).unwrap();
        std::fs::copy(&images[0], owned).unwrap();
        conn.execute(
            "UPDATE assets SET source_path=NULL,source_mtime=NULL WHERE id=?",
            [id],
        )
        .unwrap();
    }

    let referenced = import::run_import(
        &src,
        &images[1..],
        None,
        &cancel,
        import::ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(referenced.done, 1);

    let dst_meta = library::init_library(&base.join("dst"), "混合目标库").unwrap();
    let dst = Arc::new(library::open_session(dst_meta.clone()).unwrap());
    drop(src);

    let mut cfg = GlobalConfig::load_from(base.join("global-config.json")).unwrap();
    cfg.libraries.push(src_meta.clone());
    cfg.libraries.push(dst_meta.clone());
    let mut open_sessions = std::collections::HashMap::new();
    open_sessions.insert(dst_meta.id.clone(), dst.clone());
    let summary = transfer::merge_library_with_progress(
        &mut cfg,
        &src_meta.id,
        &dst_meta.id,
        &open_sessions,
        &|_| {},
    )
    .unwrap();
    assert_eq!(summary.assets, 2, "{summary:?}");

    let conn = dst.db.lock().unwrap();
    let filter = db::model::AssetFilter {
        page: 1,
        page_size: 10,
        ..Default::default()
    };
    let paged = db::assets::query_assets(&conn, &dst.root, &filter).unwrap();
    drop(conn);
    assert_eq!(paged.total, 2);

    let by_storage: std::collections::HashMap<&str, &db::model::Asset> =
        paged.items.iter().map(|a| (a.storage, a)).collect();
    let copy = by_storage
        .get("copy")
        .expect("Legacy managed assets retain copy storage");
    let reference = by_storage
        .get("reference")
        .expect("References retain reference storage");

    // Legacy managed bytes are present in the destination.
    assert!(Path::new(&copy.abs_path).exists());
    assert!(copy
        .abs_path
        .starts_with(dst.root.to_string_lossy().as_ref()));
    // Reference sources remain at the same external path.
    assert_eq!(
        reference.source_path.as_deref(),
        Some(images[1].to_string_lossy().as_ref())
    );
    assert!(Path::new(&reference.abs_path).exists());
}
