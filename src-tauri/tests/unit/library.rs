use super::*;

#[test]
fn read_only_session_reads_without_allowing_database_writes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("source");
    let meta = init_library(&root, "source").unwrap();

    let session = open_session_read_only(meta).unwrap();
    let conn = session.db.lock().unwrap();
    let version: String = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!version.is_empty());
    assert!(conn
        .execute(
            "INSERT INTO meta(key, value) VALUES('read_only_test', '1')",
            [],
        )
        .is_err());
}

#[test]
fn resumable_copy_skips_complete_files_and_validates_database() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    init_library(&source, "test").unwrap();
    fs::create_dir_all(source.join("images/26-09/a")).unwrap();
    fs::write(source.join("images/26-09/a/source.bin"), vec![7u8; 4096]).unwrap();
    let target = temp.path().join("target.part");
    fs::create_dir_all(target.join("images/26-09/a")).unwrap();
    fs::write(target.join("images/26-09/a/source.bin"), vec![7u8; 4096]).unwrap();
    let total = directory_size(&source).unwrap();
    let last = std::sync::Mutex::new(0);
    copy_dir_resumable(&source, &target, total, &|done, _, _| {
        *last.lock().unwrap() = done
    })
    .unwrap();
    assert_eq!(*last.lock().unwrap(), total);
    validate_library_copy(&target).unwrap();
}

#[test]
fn whole_library_migration_commits_path_after_verified_move() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let meta = init_library(&source, "test").unwrap();
    fs::create_dir_all(source.join("images/26-09/material-uid")).unwrap();
    fs::write(
        source.join("images/26-09/material-uid/source.bin"),
        b"material",
    )
    .unwrap();
    let config_path = temp.path().join("config.json");
    let mut config = GlobalConfig::load_from(config_path.clone()).unwrap();
    config.libraries.push(meta.clone());
    config.active_library_id = Some(meta.id.clone());
    config.save().unwrap();

    // Relocation must accept existing empty directories commonly returned by file pickers.
    let target = temp.path().join("selected-empty-target");
    fs::create_dir(&target).unwrap();
    migrate_library(&mut config, &meta.id, target.to_string_lossy().as_ref()).unwrap();

    assert!(!source.exists());
    assert!(target
        .join("images/26-09/material-uid/source.bin")
        .is_file());
    validate_library_copy(&target).unwrap();
    assert!(config.pending_migrations.is_empty());
    let reloaded = GlobalConfig::load_from(config_path).unwrap();
    assert_eq!(reloaded.libraries[0].path, target.to_string_lossy());
}

#[cfg(target_os = "macos")]
#[test]
fn installs_a_finder_custom_icon_for_library_folders() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("ClipSeek Library");
    fs::create_dir(&root).unwrap();

    install_library_folder_icon(&root).unwrap();
    install_library_folder_icon(&root).unwrap(); // Idempotent completion.

    assert!(root.join("Icon\r").is_file());
    assert_eq!(
        fs::read_to_string(root.join(".clipstudio-open/folder-icon-version")).unwrap(),
        "3"
    );
    let finder_info = std::process::Command::new("/usr/bin/xattr")
        .args(["-px", "com.apple.FinderInfo"])
        .arg(&root)
        .output()
        .unwrap();
    assert!(finder_info.status.success());
    let hex: String = String::from_utf8(finder_info.stdout)
        .unwrap()
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    assert_eq!(&hex[16..20], "0400");
    let resource_fork = std::process::Command::new("/usr/bin/xattr")
        .args(["-p", "com.apple.ResourceFork"])
        .arg(root.join("Icon\r"))
        .output()
        .unwrap();
    assert!(resource_fork.status.success());
    assert!(!resource_fork.stdout.is_empty());
    let icon_info = std::process::Command::new("/usr/bin/xattr")
        .args(["-px", "com.apple.FinderInfo"])
        .arg(root.join("Icon\r"))
        .output()
        .unwrap();
    let icon_hex: String = String::from_utf8(icon_info.stdout)
        .unwrap()
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .collect();
    assert_eq!(&icon_hex[16..20], "4000");
}

#[cfg(unix)]
#[test]
fn resumable_copy_rejects_symbolic_links() {
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    fs::create_dir(&source).unwrap();
    fs::write(temp.path().join("outside"), b"outside").unwrap();
    symlink(temp.path().join("outside"), source.join("unsafe-link")).unwrap();
    let error =
        copy_dir_resumable(&source, &temp.path().join("target"), 0, &|_, _, _| {}).unwrap_err();
    assert_eq!(error.code(), "library_symlink_unsupported");
}
