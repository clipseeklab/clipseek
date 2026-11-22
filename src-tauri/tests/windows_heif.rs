#![cfg(target_os = "windows")]
#[path = "support/resources.rs"]
mod test_resources;

use clip_studio_lib::{clip::ClipHandle, db, import, library, vector};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

fn fixture() -> PathBuf {
    let path = std::env::var_os("CLIPSEEK_HEIF_GRID_TEST_SAMPLE")
        .map(PathBuf::from)
        .unwrap_or_else(|| test_resources::media_dir("heic").join("synthetic-grid-2560x1440.heic"));
    assert!(
        path.is_file(),
        "Generate the tiled HEIC fixture before running Windows HEIF tests"
    );
    path
}

#[test]
fn complete_primary_pixels_and_png_cover_every_tile() {
    let decoded = import::thumb::open_image_smart(&fixture())
        .unwrap()
        .to_rgba8();
    assert_eq!(decoded.dimensions(), (2560, 1440));
    for row in 0..3 {
        for col in 0..5 {
            let pixel = decoded.get_pixel(col * 512 + 200, row * 512 + 200);
            let expected = [48 + col * 40, 48 + row * 70, 64 + (row * 5 + col) * 10];
            for channel in 0..3 {
                assert!((i32::from(pixel[channel]) - expected[channel] as i32).abs() <= 4);
            }
            assert_eq!(pixel[3], 255);
        }
    }
    let preview = import::thumb::full_heif_preview_png(&fixture()).unwrap();
    let preview = image::load_from_memory_with_format(&preview, image::ImageFormat::Png).unwrap();
    assert_eq!(preview.to_rgba8(), decoded);
}

#[test]
fn unicode_and_long_paths_are_decoded_without_touching_source() {
    let temp = tempfile::tempdir().unwrap();
    let mut root = temp.path().to_path_buf();
    while root.to_string_lossy().encode_utf16().count() < 280 {
        root = root.join("中文深层目录 with spaces");
    }
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("完整照片.heif");
    let original = std::fs::read(fixture()).unwrap();
    std::fs::write(&source, &original).unwrap();
    assert_eq!(
        import::thumb::image_dimensions(&source).unwrap(),
        (2560, 1440)
    );
    assert_eq!(std::fs::read(source).unwrap(), original);
    std::fs::remove_dir_all(std::fs::canonicalize(temp.path()).unwrap()).unwrap();
}

#[test]
fn truncated_input_is_rejected_and_following_decode_still_works() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("truncated.heic");
    let mut data = std::fs::read(fixture()).unwrap();
    data.truncate(data.len() - 100);
    std::fs::write(&source, data).unwrap();
    assert!(import::thumb::open_image_smart(&source).is_err());
    assert_eq!(
        import::thumb::image_dimensions(&fixture()).unwrap(),
        (2560, 1440)
    );
}

#[test]
fn import_preview_embedding_search_and_reopen_preserve_complete_image() {
    let temp = tempfile::tempdir().unwrap();
    let meta = library::init_library(temp.path(), "HEIC integration").unwrap();
    let session = Arc::new(library::open_session(meta.clone()).unwrap());
    let source = fixture();
    let original = std::fs::read(&source).unwrap();
    let summary = import::run_import(
        &session,
        std::slice::from_ref(&source),
        None,
        &AtomicBool::new(false),
        import::ImportMode::Copy,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!((summary.done, summary.failed), (1, 0));
    let asset = {
        let connection = session.db.lock().unwrap();
        db::assets::query_assets(
            &connection,
            &session.root,
            &db::model::AssetFilter {
                page: 1,
                page_size: 10,
                ..Default::default()
            },
        )
        .unwrap()
        .items
        .remove(0)
    };
    assert_eq!((asset.width, asset.height), (Some(2560), Some(1440)));
    assert_eq!(std::fs::read(&asset.abs_path).unwrap(), original);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(
        image::image_dimensions(asset.thumb_path.as_ref().unwrap()).unwrap(),
        (512, 288)
    );
    let preview =
        import::thumb::full_heif_preview_png(std::path::Path::new(&asset.abs_path)).unwrap();
    assert_eq!(
        image::load_from_memory(&preview)
            .unwrap()
            .to_rgba8()
            .dimensions(),
        (2560, 1440)
    );
    let clip = ClipHandle::default().get_or_init().unwrap();
    assert_eq!(vector::embed_pending(&session, &clip, &|_| {}).unwrap(), 1);
    vector::optimize_index(&session).unwrap();
    let query = clip
        .encode_images(&[import::thumb::open_image_smart(&source).unwrap()])
        .unwrap()
        .remove(0);
    let hits = vector::search(&session, query.clone(), None, 10).unwrap();
    assert_eq!(hits.len(), 1);
    assert!(
        hits[0].score > 0.98,
        "Image embedding must represent the complete primary image"
    );
    assert_eq!(vector::reconcile_embedding_state(&session).unwrap(), 0);
    drop(clip);
    drop(session);
    let reopened = library::open_session(meta).unwrap();
    assert_eq!(vector::search(&reopened, query, None, 10).unwrap().len(), 1);
}

#[test]
fn installer_contains_decoder_dlls_and_licenses() {
    let config: serde_json::Value = serde_json::from_slice(
        &std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tauri.windows.conf.json"))
            .unwrap(),
    )
    .unwrap();
    let resources = config["bundle"]["resources"].as_object().unwrap();
    for target in [
        "heif-decoder.exe",
        "heif.dll",
        "libde265.dll",
        "licenses/libheif-COPYING",
        "licenses/libde265-COPYING",
    ] {
        let (source, _) = resources
            .iter()
            .find(|(_, value)| value.as_str() == Some(target))
            .unwrap();
        assert!(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(source)
            .is_file());
    }
}
