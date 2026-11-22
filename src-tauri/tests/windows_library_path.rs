#![cfg(target_os = "windows")]

use std::fs;

use clip_studio_lib::config::GlobalConfig;
use clip_studio_lib::library::{import_library, init_library};

#[test]
fn imported_library_uses_familiar_windows_path_and_rejects_duplicate() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Library");
    init_library(&root, "Library").unwrap();
    let config_path = temp.path().join("config.json");
    let mut config = GlobalConfig::load_from(config_path.clone()).unwrap();

    let imported = import_library(&mut config, root.to_str().unwrap()).unwrap();
    let expected = fs::canonicalize(&root)
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    assert_eq!(imported.path, expected);
    assert!(!imported.path.starts_with(r"\\?\"));
    assert!(import_library(&mut config, root.to_str().unwrap()).is_err());

    config.libraries[0].path = fs::canonicalize(&root)
        .unwrap()
        .to_string_lossy()
        .into_owned();
    config.save().unwrap();
    let reloaded = GlobalConfig::load_from(config_path.clone()).unwrap();
    assert_eq!(reloaded.libraries[0].path, imported.path);
    let persisted: GlobalConfig =
        serde_json::from_str(&fs::read_to_string(config_path).unwrap()).unwrap();
    assert_eq!(persisted.libraries[0].path, imported.path);
}
