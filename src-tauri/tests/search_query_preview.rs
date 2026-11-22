//! Query previews are decoded from the original and return browser-readable PNG bytes.
#[test]
fn image_query_preview_is_png_with_bounded_dimensions_and_keeps_the_source() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("photo.png");
    image::RgbImage::from_pixel(400, 200, image::Rgb([220, 30, 40]))
        .save(&source)
        .unwrap();
    let before = std::fs::read(&source).unwrap();
    let bytes = clip_studio_lib::commands::search::query_image_preview(&source).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!((image.width(), image.height()), (128, 64));
    assert_eq!(std::fs::read(source).unwrap(), before);
}
