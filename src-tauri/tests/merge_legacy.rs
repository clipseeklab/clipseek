//! Merge real v16 sources without opening them in the current application first.
use clip_studio_lib::config::{GlobalConfig, LibraryMeta};
use clip_studio_lib::{db, library, transfer};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

struct Fixture {
    _temp: tempfile::TempDir,
    config: GlobalConfig,
    source: LibraryMeta,
    target: LibraryMeta,
    external: PathBuf,
}

fn version(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT CAST(value AS INTEGER) FROM meta WHERE key='schema_version'",
        [],
        |row| row.get(0),
    )
    .unwrap()
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        let dirs = library::LibraryDirs::new(&root);
        std::fs::create_dir_all(dirs.app()).unwrap();
        let conn = Connection::open(dirs.db()).unwrap();
        conn.execute_batch(include_str!("fixtures/library_v16.sql"))
            .unwrap();
        conn.execute("INSERT INTO folders VALUES(1,'Design',1)", [])
            .unwrap();
        conn.execute("INSERT INTO tags VALUES(1,'Original tag',1)", [])
            .unwrap();
        let external = temp.path().join("external.png");
        image::RgbImage::from_pixel(8, 8, image::Rgb([80, 120, 160]))
            .save(&external)
            .unwrap();
        for (id, uid) in [(1, "legacy-copy"), (2, "legacy-reference")] {
            let directory = dirs.asset(1, uid);
            std::fs::create_dir_all(&directory).unwrap();
            let original = directory.join("original.png");
            let thumb = directory.join("thumbnail.png");
            std::fs::copy(&external, &thumb).unwrap();
            if id == 1 {
                std::fs::copy(&external, &original).unwrap();
            }
            let relative = |path: &Path| {
                path.strip_prefix(&root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            };
            conn.execute(
                "INSERT INTO assets(id,uid,folder_id,file_name,rel_path,file_hash,file_size,
                 mime_type,kind,imported_at,import_stage,thumb_rel,source_path,display_name)
                 VALUES(?1,?2,1,?2 || '.png',?3,?2,100,'image/png','image',1,'done',?4,?5,?2)",
                params![
                    id,
                    uid,
                    relative(&original),
                    relative(&thumb),
                    (id == 2).then(|| external.to_string_lossy().into_owned())
                ],
            )
            .unwrap();
            conn.execute("INSERT INTO asset_tags VALUES(?1,1)", [id])
                .unwrap();
        }
        drop(conn);
        let source = LibraryMeta {
            id: "legacy-source".into(),
            name: "Legacy source".into(),
            path: root.to_string_lossy().into_owned(),
            created_at: 1,
            color: None,
        };
        library::write_library_config(&root, &source).unwrap();
        let target = library::init_library(&temp.path().join("target"), "Target").unwrap();
        let mut config = GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
        config.libraries.extend([source.clone(), target.clone()]);
        Self {
            _temp: temp,
            config,
            source,
            target,
            external,
        }
    }

    fn source_db(&self) -> Connection {
        Connection::open(library::LibraryDirs::new(Path::new(&self.source.path)).db()).unwrap()
    }

    fn target_db(&self) -> Connection {
        Connection::open(library::LibraryDirs::new(Path::new(&self.target.path)).db()).unwrap()
    }

    fn backups(&self) -> Vec<PathBuf> {
        let directory = Path::new(&self.source.path).join(".clipstudio-open/backups");
        if !directory.exists() {
            return Vec::new();
        }
        std::fs::read_dir(directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect()
    }

    fn merge(&mut self) -> clip_studio_lib::error::AppResult<transfer::TransferSummary> {
        transfer::merge_library(
            &mut self.config,
            &self.source.id,
            &self.target.id,
            &Default::default(),
        )
    }
}

#[test]
fn unopened_v16_source_is_backed_up_and_upgraded_before_copying() {
    let mut fixture = Fixture::new();
    let observed = AtomicBool::new(false);
    let source_path = fixture.source.path.clone();
    let summary = transfer::merge_library_with_progress(
        &mut fixture.config,
        &fixture.source.id,
        &fixture.target.id,
        &Default::default(),
        &|progress| {
            if progress.phase == "checking_space" {
                let source =
                    Connection::open(Path::new(&source_path).join(".clipstudio-open/library.db"))
                        .unwrap();
                assert_eq!(version(&source), db::SCHEMA_VERSION);
                let backups: Vec<_> =
                    std::fs::read_dir(Path::new(&source_path).join(".clipstudio-open/backups"))
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .collect();
                assert_eq!(backups.len(), 1);
                let backup = Connection::open(&backups[0]).unwrap();
                assert_eq!(version(&backup), 16);
                assert_eq!(
                    backup
                        .query_row("SELECT COUNT(*) FROM assets", [], |row| row
                            .get::<_, i64>(0))
                        .unwrap(),
                    2
                );
                observed.store(true, Ordering::SeqCst);
            }
        },
    )
    .unwrap();
    assert!(observed.load(Ordering::SeqCst));
    assert_eq!(summary.assets, 2);
    assert_eq!(summary.failed, 0);
    assert!(!Path::new(&fixture.source.path).exists());
    assert!(fixture.external.is_file());
    let session = library::open_session(fixture.target.clone()).unwrap();
    let assets =
        db::assets::query_assets(&session.read_db(), &session.root, &Default::default()).unwrap();
    assert_eq!(assets.total, 2);
    for asset in assets.items {
        assert_eq!(asset.folder_name.as_deref(), Some("Design"));
        assert_eq!(asset.tags, ["Original tag"]);
        assert_eq!(asset.display_name.as_deref(), Some(asset.uid.as_str()));
        assert!(!asset.subtitle_available);
        assert!(Path::new(&asset.abs_path).is_file());
        assert_eq!(
            asset.storage,
            if asset.uid == "legacy-copy" {
                "copy"
            } else {
                "reference"
            }
        );
    }
}

#[test]
fn failed_copy_retains_upgraded_source_and_one_original_backup_for_retry() {
    let mut fixture = Fixture::new();
    fixture.target_db().execute_batch("CREATE TRIGGER reject_copy BEFORE INSERT ON assets BEGIN SELECT RAISE(ABORT,'test copy failure'); END;").unwrap();
    let error = fixture.merge().unwrap_err();
    assert!(error.to_string().contains("format upgraded"), "{error}");
    assert_eq!(version(&fixture.source_db()), db::SCHEMA_VERSION);
    assert!(fixture.external.is_file());
    let backups = fixture.backups();
    assert_eq!(backups.len(), 1);
    assert_eq!(version(&Connection::open(&backups[0]).unwrap()), 16);
    let bytes = std::fs::read(&backups[0]).unwrap();
    assert!(fixture.merge().is_err());
    assert_eq!(fixture.backups(), backups);
    assert_eq!(std::fs::read(&backups[0]).unwrap(), bytes);
    fixture
        .target_db()
        .execute_batch("DROP TRIGGER reject_copy")
        .unwrap();
    assert_eq!(fixture.merge().unwrap().assets, 2);
    assert!(!Path::new(&fixture.source.path).exists());
}

#[test]
fn failed_backup_stops_merge_without_upgrading_source_or_copying_to_target() {
    let mut fixture = Fixture::new();
    let directory = Path::new(&fixture.source.path).join(".clipstudio-open/backups");
    std::fs::write(&directory, b"blocked backup destination").unwrap();
    assert!(fixture.merge().is_err());
    assert_eq!(version(&fixture.source_db()), 16);
    assert_eq!(fixture.config.libraries.len(), 2);
    assert_eq!(
        fixture
            .target_db()
            .query_row("SELECT COUNT(*) FROM assets", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert!(fixture.external.is_file());
}

#[test]
fn trash_is_rejected_before_either_library_is_upgraded() {
    let mut fixture = Fixture::new();
    fixture
        .source_db()
        .execute("UPDATE assets SET deleted_at=1 WHERE id=1", [])
        .unwrap();
    let error = fixture.merge().unwrap_err();
    assert!(error.to_string().contains("trashed"));
    assert_eq!(version(&fixture.source_db()), 16);
    assert!(fixture.backups().is_empty());
    assert!(Path::new(&fixture.source.path).is_dir());
}

#[test]
fn future_target_is_rejected_before_source_upgrade() {
    let mut fixture = Fixture::new();
    fixture
        .target_db()
        .execute(
            "UPDATE meta SET value=?1 WHERE key='schema_version'",
            [db::SCHEMA_VERSION + 1],
        )
        .unwrap();
    assert_eq!(
        fixture.merge().unwrap_err().code(),
        "library_upgrade_required"
    );
    assert_eq!(version(&fixture.source_db()), 16);
    assert!(fixture.backups().is_empty());
}
