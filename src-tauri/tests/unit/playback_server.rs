use super::playback_http_url;

#[tokio::test]
async fn local_playback_server_streams_byte_ranges() {
    let temp = tempfile::tempdir().unwrap();
    let video = temp.path().join("sample.mp4");
    std::fs::write(&video, b"0123456789").unwrap();
    let url = playback_http_url(&video, "video/mp4").unwrap();
    let response = reqwest::Client::builder()
        .no_proxy()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .unwrap()
        .get(url)
        .header(reqwest::header::RANGE, "bytes=2-5")
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), reqwest::StatusCode::PARTIAL_CONTENT);
    assert_eq!(
        response.headers()[reqwest::header::CONTENT_TYPE],
        "video/mp4"
    );
    assert_eq!(response.headers()[reqwest::header::ACCEPT_RANGES], "bytes");
    assert_eq!(response.bytes().await.unwrap().as_ref(), b"2345");
}

#[tokio::test]
async fn reference_playback_retains_its_library_only_while_a_response_is_alive() {
    use std::sync::Arc;

    use axum::{body::Body, http::Request, routing::get, Router};
    use tower::ServiceExt;

    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("library");
    let meta = crate::library::init_library(&root, "Reference playback").unwrap();
    let session = Arc::new(crate::library::open_session_light(meta).unwrap());
    session.asset_activity.bind_session(&session);
    let weak = Arc::downgrade(&session);
    // A reference source is outside the library, so path lookup alone cannot retain it.
    let source = directory.path().join("external.mp4");
    std::fs::write(&source, b"0123456789").unwrap();
    let url = super::playback_url_for_asset(&session, &source, "video/mp4").unwrap();
    let registry = super::playback_http_server().unwrap().downloads.clone();
    let response = Router::new()
        .route(
            "/files/{download_id}/{file_name}",
            get(crate::playback_media::download),
        )
        .with_state(registry)
        .oneshot(Request::builder().uri(url).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    drop(session);
    assert!(weak.upgrade().is_some());
    drop(response);
    assert!(weak.upgrade().is_none());
    assert_eq!(std::fs::read(&source).unwrap(), b"0123456789");
}
