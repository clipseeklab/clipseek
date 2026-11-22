#![cfg(target_os = "windows")]

use std::os::windows::fs::MetadataExt;

use clip_studio_lib::library::install_library_folder_icon;

#[test]
fn library_folder_has_a_portable_explorer_icon() {
    const READONLY: u32 = 0x1;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;

    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("中文素材库");
    std::fs::create_dir(&root).unwrap();

    install_library_folder_icon(&root).unwrap();
    install_library_folder_icon(&root).unwrap();

    let icon = root.join(".clipstudio-open/library-folder.ico");
    assert!(icon.is_file());
    assert!(std::fs::metadata(&icon).unwrap().len() > 0);

    let ini = root.join("desktop.ini");
    let bytes = std::fs::read(&ini).unwrap();
    assert!(bytes.starts_with(&[0xff, 0xfe]));
    let text = String::from_utf16(
        &bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert!(text.contains("IconFile=.clipstudio-open\\library-folder.ico"));
    assert!(text.contains("IconIndex=0"));
    assert_eq!(
        std::fs::metadata(&ini).unwrap().file_attributes() & (HIDDEN | SYSTEM),
        HIDDEN | SYSTEM
    );
    assert_ne!(
        std::fs::metadata(&root).unwrap().file_attributes() & READONLY,
        0
    );
}

#[test]
fn existing_user_desktop_ini_is_preserved() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("My Library");
    std::fs::create_dir(&root).unwrap();
    let ini = root.join("desktop.ini");
    let custom = b"[.ShellClassInfo]\r\nIconResource=custom.ico,0\r\n";
    std::fs::write(&ini, custom).unwrap();

    install_library_folder_icon(&root).unwrap();

    assert_eq!(std::fs::read(&ini).unwrap(), custom);
    assert!(!root.join(".clipstudio-open/library-folder.ico").exists());
}
