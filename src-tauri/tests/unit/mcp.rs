use super::server::{authorize, validate_network_headers, AuthState, NetworkState};
use super::tools::SearchFilters;
use super::*;
use axum::{
    body::Body,
    extract::Request,
    http::{header, StatusCode},
    middleware,
    routing::get,
};
use tower::ServiceExt;
#[test]
fn uid_validation_rejects_paths() {
    assert!(validate_uid("../../secret").is_err());
    assert!(validate_uid(&uuid::Uuid::now_v7().to_string()).is_ok());
}
#[test]
fn default_search_filter_is_safe() {
    let filter = SearchFilters::default().into_filter(None, 1, 20);
    assert!(!filter.in_trash);
    assert_eq!(filter.page_size, 20);
}
#[tokio::test]
async fn rate_limit_returns_429_once_threshold_is_reached() {
    let minute = crate::util::now() / 60;
    let mut rate = HashMap::new();
    rate.insert("127.0.0.1".to_string(), (minute, MCP_REQUESTS_PER_MINUTE));
    let auth = AuthState {
        token: Arc::new(RwLock::new("secret".into())),
        rate: Arc::new(Mutex::new(rate)),
    };
    let app = axum::Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(auth, authorize));
    let mut request = Request::builder()
        .uri("/")
        .header(header::AUTHORIZATION, "Bearer secret")
        .body(Body::empty())
        .unwrap();
    request.extensions_mut().insert(axum::extract::ConnectInfo(
        "127.0.0.1:5000".parse::<std::net::SocketAddr>().unwrap(),
    ));
    let response = app.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}
#[tokio::test]
async fn network_headers_reject_unknown_host_and_origin() {
    fn request(host: &str, origin: Option<&str>, remote: &str) -> Request<Body> {
        let mut builder = Request::builder().uri("/").header(header::HOST, host);
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        let mut request = builder.body(Body::empty()).unwrap();
        request.extensions_mut().insert(axum::extract::ConnectInfo(
            remote.parse::<std::net::SocketAddr>().unwrap(),
        ));
        request
    }
    let network = NetworkState {
        hosts: Arc::new(vec!["127.0.0.1".into(), "192.168.1.8".into()]),
        origins: Arc::new(vec!["http://192.168.1.8:3210".into()]),
        lan_enabled: true,
    };
    let app = axum::Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(
            network,
            validate_network_headers,
        ));
    let bad_host = app
        .clone()
        .oneshot(request("evil.example", None, "192.168.1.20:5000"))
        .await
        .unwrap();
    assert_eq!(bad_host.status(), StatusCode::FORBIDDEN);
    let bad_origin = app
        .clone()
        .oneshot(request(
            "192.168.1.8",
            Some("https://evil.example"),
            "192.168.1.20:5000",
        ))
        .await
        .unwrap();
    assert_eq!(bad_origin.status(), StatusCode::FORBIDDEN);
    let public_source = app
        .clone()
        .oneshot(request("192.168.1.8", None, "203.0.113.20:5000"))
        .await
        .unwrap();
    assert_eq!(public_source.status(), StatusCode::FORBIDDEN);
    let allowed = app
        .oneshot(request(
            "192.168.1.8",
            Some("http://192.168.1.8:3210"),
            "192.168.1.20:5000",
        ))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
}
#[tokio::test]
async fn bearer_token_is_required() {
    let auth = AuthState {
        token: Arc::new(RwLock::new("secret".into())),
        rate: Arc::new(Mutex::new(HashMap::new())),
    };
    let app = axum::Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(auth, authorize));
    let unauthorized = app
        .clone()
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let authorized = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header(header::AUTHORIZATION, "Bearer secret")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(authorized.status(), StatusCode::OK);
}
