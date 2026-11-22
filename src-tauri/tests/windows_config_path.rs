#![cfg(target_os = "windows")]

use std::path::PathBuf;

use clip_studio_lib::config::GlobalConfig;

#[test]
fn global_config_uses_roaming_app_data_on_windows() {
    let app_data = std::env::var_os("APPDATA").expect("Windows roaming AppData path");
    let expected = PathBuf::from(app_data)
        .join("org.clipstudio.open")
        .join("config.json");

    assert_eq!(GlobalConfig::path().unwrap(), expected);
}
