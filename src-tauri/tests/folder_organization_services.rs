//! Folder organization contracts shared by desktop commands and MCP services.

use std::sync::{atomic::AtomicBool, Arc};

use clip_studio_lib::db::{self, model::Asset};
use clip_studio_lib::import::{self, ImportMode};
use clip_studio_lib::library::{self, LibrarySession};
use clip_studio_lib::services;
use clip_studio_lib::services::folders::{create_folder, delete_folder, rename_folder};

struct Fixture {
    session: Arc<LibrarySession>,
    temp: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let meta = library::init_library(&temp.path().join("library"), "Folders").unwrap();
        Self {
            session: Arc::new(library::open_session(meta).unwrap()),
            temp,
        }
    }

    fn reference(&self, folder: i64) -> Asset {
        let source = self.temp.path().join("external.png");
        image::DynamicImage::new_rgb8(8, 8).save(&source).unwrap();
        let batch = import::register_files(
            &self.session,
            &[source],
            Some(folder),
            &AtomicBool::new(false),
            ImportMode::Reference,
            &|_, _, _| {},
            &|_| {},
        );
        assert_eq!(batch.pending.len(), 1);
        self.asset(batch.pending[0].0)
    }

    fn asset(&self, id: i64) -> Asset {
        db::assets::get_asset(&self.session.read_db(), &self.session.root, id)
            .unwrap()
            .unwrap()
    }
}

#[test]
fn creation_auto_pins_up_to_the_display_limit_after_an_empty_sidebar_was_saved() {
    for limit in [5, 10, 20, 30] {
        let fixture = Fixture::new();
        db::folders::set_sidebar_with_limit(&fixture.session.db.lock().unwrap(), &[], Some(limit))
            .unwrap();
        let mut ids = Vec::new();
        for number in 1..=limit + 1 {
            let folder = create_folder(&fixture.session, &format!("Folder {number:02}")).unwrap();
            ids.push(folder.id);
            let sidebar = db::folders::sidebar(&fixture.session.db.lock().unwrap()).unwrap();
            let expected = ids
                .iter()
                .take(limit as usize)
                .rev()
                .copied()
                .collect::<Vec<_>>();
            assert_eq!(
                sidebar.items.iter().map(|item| item.id).collect::<Vec<_>>(),
                expected,
                "limit={limit}, total={number}",
            );
            assert_eq!(sidebar.total, number);
            assert_eq!(sidebar.display_limit, limit);
        }
        // Falling below the threshold again enables pinning for subsequent creations.
        delete_folder(&fixture.session, ids[0]).unwrap();
        delete_folder(&fixture.session, ids[1]).unwrap();
        let folder = create_folder(&fixture.session, "After deletion").unwrap();
        let sidebar = db::folders::sidebar(&fixture.session.db.lock().unwrap()).unwrap();
        assert_eq!(sidebar.total, limit);
        assert_eq!(sidebar.items[0].id, folder.id);
    }
}

#[test]
fn creation_auto_pin_immediately_follows_display_limit_changes() {
    let fixture = Fixture::new();
    for number in 0..5 {
        create_folder(&fixture.session, &format!("Folder {number}")).unwrap();
    }
    let ids = db::folders::sidebar(&fixture.session.db.lock().unwrap())
        .unwrap()
        .items
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    db::folders::set_sidebar_with_limit(&fixture.session.db.lock().unwrap(), &ids, Some(20))
        .unwrap();
    let sixth = create_folder(&fixture.session, "Sixth").unwrap();
    let sidebar = db::folders::sidebar(&fixture.session.db.lock().unwrap()).unwrap();
    assert_eq!(sidebar.items[0].id, sixth.id);
    let pins = sidebar.items.iter().map(|item| item.id).collect::<Vec<_>>();
    db::folders::set_sidebar_with_limit(&fixture.session.db.lock().unwrap(), &pins, Some(5))
        .unwrap();
    create_folder(&fixture.session, "Seventh").unwrap();
    let sidebar = db::folders::sidebar(&fixture.session.db.lock().unwrap()).unwrap();
    assert_eq!(
        sidebar.items.iter().map(|item| item.id).collect::<Vec<_>>(),
        pins
    );
}

#[test]
fn creation_auto_pin_preserves_manual_unpins_and_existing_order() {
    let fixture = Fixture::new();
    let first = create_folder(&fixture.session, "First").unwrap();
    let second = create_folder(&fixture.session, "Second").unwrap();
    let third = create_folder(&fixture.session, "Third").unwrap();
    db::folders::set_sidebar(&fixture.session.db.lock().unwrap(), &[first.id, third.id]).unwrap();
    let fourth = create_folder(&fixture.session, "Fourth").unwrap();
    let ids = db::folders::sidebar(&fixture.session.db.lock().unwrap())
        .unwrap()
        .items
        .iter()
        .map(|item| item.id)
        .collect::<Vec<_>>();
    assert_eq!(ids, vec![fourth.id, first.id, third.id]);
    assert!(!ids.contains(&second.id));
}

#[test]
fn creation_auto_pin_uses_total_folder_count_instead_of_pinned_count() {
    let fixture = Fixture::new();
    for number in 0..10 {
        create_folder(&fixture.session, &format!("Folder {number}")).unwrap();
    }
    db::folders::set_sidebar(&fixture.session.db.lock().unwrap(), &[]).unwrap();
    create_folder(&fixture.session, "Eleventh").unwrap();
    assert!(db::folders::sidebar(&fixture.session.db.lock().unwrap())
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn creation_auto_pin_failure_rolls_back_the_new_folder() {
    let fixture = Fixture::new();
    {
        let conn = fixture.session.db.lock().unwrap();
        db::folders::set_sidebar(&conn, &[]).unwrap();
        conn.execute_batch("CREATE TRIGGER reject_pin_update BEFORE UPDATE ON meta WHEN NEW.key='folder_sidebar_ids.v1' BEGIN SELECT RAISE(ABORT,'pin write failed'); END;").unwrap();
    }
    assert!(create_folder(&fixture.session, "Should roll back").is_err());
    assert!(db::folders::list(&fixture.session.read_db())
        .unwrap()
        .is_empty());
    assert!(db::folders::sidebar(&fixture.session.db.lock().unwrap())
        .unwrap()
        .items
        .is_empty());
}

#[test]
fn mcp_creation_trims_valid_names_and_rejects_invalid_names_without_writes() {
    let fixture = Fixture::new();
    let created = services::create_folder(&fixture.session, "  Target  ").unwrap();
    assert_eq!(created.name, "Target");
    for name in ["", "  ", "a/b", "a:b", ".hidden"] {
        assert_eq!(
            services::create_folder(&fixture.session, name)
                .unwrap_err()
                .code(),
            "invalid_argument"
        );
    }
    assert_eq!(
        db::folders::list(&fixture.session.read_db()).unwrap().len(),
        1
    );
}

#[test]
fn renaming_a_folder_updates_metadata_without_moving_reference_sources() {
    let fixture = Fixture::new();
    let folder = create_folder(&fixture.session, "Before").unwrap();
    let original = fixture.reference(folder.id);
    rename_folder(&fixture.session, folder.id, "After").unwrap();
    let renamed = fixture.asset(original.id);
    assert_eq!(renamed.folder_name.as_deref(), Some("After"));
    assert_eq!(renamed.folder_id, Some(folder.id));
    assert_eq!(renamed.abs_path, original.abs_path);
    assert!(std::path::Path::new(&original.abs_path).is_file());
    rename_folder(&fixture.session, folder.id, "After").unwrap();
    assert_eq!(fixture.asset(original.id).folder_id, Some(folder.id));
}

#[test]
fn deleting_a_folder_trashes_its_assets_and_preserves_reference_sources() {
    let fixture = Fixture::new();
    let folder = create_folder(&fixture.session, "Delete").unwrap();
    let original = fixture.reference(folder.id);
    delete_folder(&fixture.session, folder.id).unwrap();
    assert!(db::folders::get_name(&fixture.session.read_db(), folder.id)
        .unwrap()
        .is_none());
    let trashed = fixture.asset(original.id);
    assert!(trashed.deleted_at.is_some());
    assert!(trashed.folder_id.is_none());
    assert_eq!(trashed.abs_path, original.abs_path);
    assert!(std::path::Path::new(&original.abs_path).is_file());
}

#[test]
fn unknown_folder_errors_keep_stable_codes() {
    let fixture = Fixture::new();
    assert_eq!(
        rename_folder(&fixture.session, 999, "After")
            .unwrap_err()
            .code(),
        "folder_not_found"
    );
    assert_eq!(
        delete_folder(&fixture.session, 999).unwrap_err().code(),
        "folder_not_found"
    );
}

#[test]
fn batch_move_validates_all_folders_before_mutation_and_keeps_source_folders() {
    let fixture = Fixture::new();
    let source = create_folder(&fixture.session, "Source").unwrap();
    let target = create_folder(&fixture.session, "Target").unwrap();
    let original = fixture.reference(source.id);
    for (ids, destination, expected) in [
        (vec![source.id, 999], Some(target.id), "folder_not_found"),
        (vec![source.id], Some(999), "folder_not_found"),
        (vec![source.id], Some(source.id), "invalid_argument"),
        (vec![], Some(target.id), "invalid_argument"),
        (vec![source.id; 1001], Some(target.id), "invalid_argument"),
    ] {
        assert_eq!(
            services::folders::batch(&fixture.session, &ids, destination)
                .unwrap_err()
                .code(),
            expected
        );
        assert_eq!(fixture.asset(original.id).folder_id, Some(source.id));
    }
    services::folders::batch(&fixture.session, &[source.id, source.id], Some(target.id)).unwrap();
    assert_eq!(fixture.asset(original.id).folder_id, Some(target.id));
    assert!(db::folders::get_name(&fixture.session.read_db(), source.id)
        .unwrap()
        .is_some());
    assert_eq!(fixture.asset(original.id).abs_path, original.abs_path);
    assert!(std::path::Path::new(&original.abs_path).is_file());
}

#[test]
fn batch_delete_deduplicates_folders_and_preserves_reference_sources() {
    let fixture = Fixture::new();
    let source = create_folder(&fixture.session, "Delete").unwrap();
    let original = fixture.reference(source.id);
    services::folders::batch(&fixture.session, &[source.id, source.id], None).unwrap();
    assert!(db::folders::get_name(&fixture.session.read_db(), source.id)
        .unwrap()
        .is_none());
    assert!(fixture.asset(original.id).deleted_at.is_some());
    assert!(std::path::Path::new(&original.abs_path).is_file());
}
