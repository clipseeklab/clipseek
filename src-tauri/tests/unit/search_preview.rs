use super::*;

#[test]
fn query_preview_is_a_small_browser_decodable_png() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("query.png");
    image::DynamicImage::new_rgb8(640, 480)
        .save(&source)
        .unwrap();
    let bytes = query_image_preview(&source).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let decoded = image::load_from_memory(&bytes).unwrap();
    assert_eq!((decoded.width(), decoded.height()), (128, 96));
}

#[test]
fn query_preview_rejects_video_before_reading_it() {
    let error = query_image_preview(std::path::Path::new("missing-large-video.mkv")).unwrap_err();
    assert!(error
        .to_string()
        .contains("Search preview requires an image"));
}
