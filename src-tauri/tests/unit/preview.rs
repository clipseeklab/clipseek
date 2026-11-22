use super::*;

fn request(path: &Path, method: Method) -> Request<Vec<u8>> {
    let encoded = percent_encoding::utf8_percent_encode(
        &path.to_string_lossy(),
        percent_encoding::NON_ALPHANUMERIC,
    )
    .to_string();
    Request::builder()
        .method(method)
        .uri(format!("asset://localhost/{encoded}"))
        .body(Vec::new())
        .unwrap()
}

#[test]
fn video_without_thumbnail_is_unavailable_in_every_import_stage() {
    assert_eq!(
        asset_path("video", None, "big.mkv".into())
            .unwrap_err()
            .code(),
        "preview_unavailable"
    );
    assert_eq!(
        asset_path("video", Some("cover.webp".into()), "big.mkv".into()).unwrap(),
        "cover.webp"
    );
}

#[test]
fn original_video_is_rejected_even_when_small_or_requested_with_range() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("video.mkv");
    std::fs::write(&path, b"video").unwrap();
    assert_eq!(
        read_image(&path).unwrap_err().code(),
        "preview_type_unsupported"
    );
    let mut req = request(&path, Method::GET);
    req.headers_mut()
        .insert("range", "bytes=0-1".parse().unwrap());
    assert_eq!(
        response(req, |_| true).status(),
        StatusCode::UNSUPPORTED_MEDIA_TYPE
    );
}

#[test]
fn oversized_image_is_rejected_before_reading() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("large.jpg");
    File::create(&path)
        .unwrap()
        .set_len(MAX_PREVIEW_BYTES + 1)
        .unwrap();
    assert_eq!(read_image(&path).unwrap_err().code(), "preview_too_large");
    let res = response(request(&path, Method::GET), |_| true);
    assert_eq!(res.status(), StatusCode::PAYLOAD_TOO_LARGE);
    assert_eq!(res.body(), b"preview_too_large");
}

#[test]
fn growing_input_is_bounded_independently_of_metadata() {
    let reader = std::io::repeat(0).take(MAX_PREVIEW_BYTES + 1024);
    assert_eq!(
        read_bounded(reader).unwrap_err().code(),
        "preview_too_large"
    );
}

#[test]
fn scoped_images_and_head_work_but_outside_paths_do_not() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("封面.webp");
    std::fs::write(&path, b"image").unwrap();
    let res = response(request(&path, Method::GET), |p| p == path);
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.body(), b"image");
    assert_eq!(res.headers()["content-type"], "image/webp");
    let head = response(request(&path, Method::HEAD), |_| true);
    assert_eq!(head.headers()["content-length"], "5");
    assert!(head.body().is_empty());
    assert_eq!(
        response(request(&path, Method::GET), |_| false).status(),
        StatusCode::FORBIDDEN
    );
}

#[test]
fn missing_preview_returns_not_found_without_falling_back() {
    let temp = tempfile::tempdir().unwrap();
    assert_eq!(
        response(
            request(&temp.path().join("missing.webp"), Method::GET),
            |_| true
        )
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn absolute_unix_paths_keep_the_leading_slash() {
    let req = Request::builder()
        .uri("asset://localhost//missing-cover.webp")
        .body(Vec::new())
        .unwrap();
    let res = response(req, |path| path == Path::new("/missing-cover.webp"));
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[test]
fn preview_size_limit_has_an_inclusive_boundary() {
    assert!(check_size(MAX_PREVIEW_BYTES).is_ok());
    assert_eq!(
        check_size(MAX_PREVIEW_BYTES + 1).unwrap_err().code(),
        "preview_too_large"
    );
}

#[test]
fn scoped_preview_refuses_a_library_under_exclusive_file_maintenance() {
    let temp = tempfile::tempdir().unwrap();
    let meta = crate::library::init_library(temp.path(), "Preview maintenance").unwrap();
    let session = crate::library::open_session(meta).unwrap();
    let path = temp.path().join("images/preview.webp");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"image").unwrap();
    let _maintenance = session.asset_activity.cleanup().unwrap();
    let res = response(request(&path, Method::GET), |_| true);
    assert_eq!(res.status(), StatusCode::CONFLICT);
}
