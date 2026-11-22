use super::*;
use tower::ServiceExt;

#[test]
fn parses_common_ranges() {
    assert_eq!(parse_range(None, 100).unwrap(), (0, 99, false));
    assert_eq!(
        parse_range(Some(&HeaderValue::from_static("bytes=10-19")), 100).unwrap(),
        (10, 19, true)
    );
    assert_eq!(
        parse_range(Some(&HeaderValue::from_static("bytes=90-")), 100).unwrap(),
        (90, 99, true)
    );
    assert_eq!(
        parse_range(Some(&HeaderValue::from_static("bytes=-10")), 100).unwrap(),
        (90, 99, true)
    );
    assert!(parse_range(Some(&HeaderValue::from_static("bytes=100-101")), 100).is_err());
    assert!(parse_range(Some(&HeaderValue::from_static("bytes=0-1,4-5")), 100).is_err());
}

#[tokio::test]
async fn streams_full_head_and_range_downloads() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.bin");
    std::fs::write(&source, b"0123456789").unwrap();
    let registry = Arc::new(DownloadRegistry::default());
    let info = registry
        .issue(
            source,
            "sample.bin".into(),
            "application/octet-stream".into(),
            "http://127.0.0.1:1234",
            false,
        )
        .unwrap();
    let url = reqwest::Url::parse(&info.url).unwrap();
    let uri = format!("{}?{}", url.path(), url.query().unwrap());
    let app = axum::Router::new()
        .route(
            "/files/{download_id}/{file_name}",
            axum::routing::get(download).head(download),
        )
        .with_state(registry);

    let ranged = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header(header::RANGE, "bytes=2-5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ranged.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(ranged.headers()[header::CONTENT_RANGE], "bytes 2-5/10");
    assert!(ranged.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("attachment;"));
    let bytes = axum::body::to_bytes(ranged.into_body(), 32).await.unwrap();
    assert_eq!(&bytes[..], b"2345");

    let head = app
        .oneshot(
            Request::builder()
                .method(Method::HEAD)
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(head.status(), StatusCode::OK);
    assert_eq!(head.headers()[header::CONTENT_LENGTH], "10");
    assert!(axum::body::to_bytes(head.into_body(), 32)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn inline_media_grant_keeps_range_support() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.mp4");
    std::fs::write(&source, b"0123456789").unwrap();
    let registry = Arc::new(DownloadRegistry::default());
    let info = registry
        .issue_inline(
            source,
            "sample.mp4".into(),
            "video/mp4".into(),
            "http://127.0.0.1:1234",
            24 * 60 * 60,
        )
        .unwrap();
    let url = reqwest::Url::parse(&info.url).unwrap();
    let uri = format!("{}?{}", url.path(), url.query().unwrap());
    let app = axum::Router::new()
        .route(
            "/files/{download_id}/{file_name}",
            axum::routing::get(download).head(download),
        )
        .with_state(registry);

    let response = app
        .oneshot(
            Request::builder()
                .uri(&uri)
                .header(header::RANGE, "bytes=2-5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "video/mp4");
    assert_eq!(response.headers()[header::ACCEPT_RANGES], "bytes");
    assert!(response.headers()[header::CONTENT_DISPOSITION]
        .to_str()
        .unwrap()
        .starts_with("inline;"));
}

#[test]
fn revocation_invalidates_capability() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("sample.bin");
    std::fs::write(&source, b"data").unwrap();
    let registry = DownloadRegistry::default();
    let info = registry
        .issue(
            source,
            "sample.bin".into(),
            "application/octet-stream".into(),
            "http://localhost",
            false,
        )
        .unwrap();
    let token = reqwest::Url::parse(&info.url)
        .unwrap()
        .query_pairs()
        .find(|(key, _)| key == "token")
        .unwrap()
        .1
        .into_owned();
    assert!(registry.get(&info.download_id, &token).is_ok());
    registry.revoke_all();
    assert!(matches!(
        registry.get(&info.download_id, &token),
        Err(AppError::DownloadNotFound)
    ));
}

#[tokio::test]
async fn playback_body_holds_file_activity_until_consumed_or_dropped() {
    for external in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let meta =
            crate::library::init_library(&temp.path().join("library"), "Playback guard").unwrap();
        let session = crate::library::open_session(meta).unwrap();
        let path = if external {
            temp.path().join("reference.mp4")
        } else {
            session.root.join("images/play.mp4")
        };
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"0123456789").unwrap();
        let registry = Arc::new(DownloadRegistry::for_local_playback());
        let info = registry
            .issue_inline(
                path,
                "play.mp4".into(),
                "video/mp4".into(),
                "http://localhost",
                60,
            )
            .unwrap();
        if external {
            registry.bind_library_activity(&info.download_id, session.asset_activity.clone());
        }
        let url = reqwest::Url::parse(&info.url).unwrap();
        let uri = format!("{}?{}", url.path(), url.query().unwrap());
        let app = axum::Router::new()
            .route(
                "/files/{download_id}/{file_name}",
                axum::routing::get(download),
            )
            .with_state(registry);
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            session.asset_activity.cleanup().is_err(),
            "a response still owns the file even before its body is polled"
        );
        drop(response);
        let maintenance = session.asset_activity.cleanup().unwrap();
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        drop(maintenance);
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(&uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let bytes = axum::body::to_bytes(response.into_body(), 100)
            .await
            .unwrap();
        assert_eq!(bytes.as_ref(), b"0123456789");
        assert!(session.asset_activity.cleanup().is_ok());
    }
}
