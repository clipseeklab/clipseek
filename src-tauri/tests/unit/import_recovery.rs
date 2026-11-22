use super::*;

#[test]
fn repairs_database_path_when_crash_happened_after_final_move() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("library");
    let meta = crate::library::init_library(&root, "recovery").unwrap();
    let session = Arc::new(crate::library::open_session(meta).unwrap());
    let dirs = LibraryDirs::new(&root);
    let uid = "01999999-9999-7999-8999-999999999999";
    let imported_at = 1_700_000_000;
    let name = "interrupted.png";
    let final_path = dirs.asset(imported_at, uid).join(name);
    fs::create_dir_all(final_path.parent().unwrap()).unwrap();
    image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(8, 8, image::Rgb([20, 80, 160])))
        .save(&final_path)
        .unwrap();

    let asset_id = {
        let conn = session.db.lock().unwrap();
        db::assets::insert_asset(
            &conn,
            crate::db::assets::NewAsset {
                folder_id: None,
                file_name: name,
                rel_path: ".clipstudio-open/importing/interrupted.png",
                file_hash: "recovery-hash",
                file_size: fs::metadata(&final_path).unwrap().len() as i64,
                mime_type: "image/png",
                kind: "image",
                imported_at,
                uid: Some(uid),
                ..Default::default()
            },
        )
        .unwrap()
    };

    process_asset(&session, &dirs, asset_id, &|_| {}).unwrap();
    let conn = session.db.lock().unwrap();
    let asset = db::assets::get_asset(&conn, &root, asset_id)
        .unwrap()
        .unwrap();
    assert_eq!(std::path::Path::new(&asset.abs_path), final_path);
    assert_eq!(asset.import_stage, "embedding");
}
