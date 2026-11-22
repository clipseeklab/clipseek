use axum::{
    body::Body,
    http::{Request, StatusCode},
    routing::get,
    Router,
};
use clip_studio_lib::playback_media::{download, DownloadRegistry};
use std::sync::Arc;
use tower::ServiceExt;

fn router(registry: Arc<DownloadRegistry>) -> Router {
    Router::new()
        .route(
            "/files/{download_id}/{file_name}",
            get(download).head(download),
        )
        .with_state(registry)
}

#[tokio::test]
async fn local_playback_range_burst_does_not_block_the_next_video() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("video.mov");
    std::fs::write(&path, b"0123456789").unwrap();
    let registry = Arc::new(DownloadRegistry::for_local_playback());
    let raw = registry
        .issue_inline(
            path.clone(),
            "raw.mov".into(),
            "video/quicktime".into(),
            "http://localhost",
            3600,
        )
        .unwrap();
    let normal = registry
        .issue_inline(
            path,
            "normal.mp4".into(),
            "video/mp4".into(),
            "http://localhost",
            3600,
        )
        .unwrap();
    let app = router(registry);
    for index in 0..180 {
        let url = if index < 150 { &raw.url } else { &normal.url };
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(url)
                    .header("range", "bytes=0-1")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::PARTIAL_CONTENT,
            "Range request {index} must remain playable"
        );
        assert_eq!(
            axum::body::to_bytes(response.into_body(), 10)
                .await
                .unwrap()
                .as_ref(),
            b"01"
        );
    }
}

#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn real_webkit_proxy_then_normal() {
    let probe = std::env::var_os("CLIPSTUDIO_WEBKIT_PROBE");
    if !clip_studio_lib::util::test_prerequisite(
        probe.is_some(),
        "Set CLIPSTUDIO_WEBKIT_PROBE to the compiled tests/native/playback-probe.swift helper",
    ) {
        return;
    }
    let probe = probe.unwrap();
    let root = test_resources::media_dir("raw-video");
    let baseline = std::env::var_os("CLIPSTUDIO_PLAYBACK_BASELINE")
        .expect("Set CLIPSTUDIO_PLAYBACK_BASELINE to a playable MP4");
    let registry = Arc::new(DownloadRegistry::for_local_playback());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let paths = [
        std::path::PathBuf::from(&baseline),
        root.join("raw-sample-a.mov"),
        std::path::PathBuf::from(&baseline),
        root.join("raw-sample-b.mov"),
        std::path::PathBuf::from(baseline),
    ];
    let file_mode = std::env::var_os("CLIPSTUDIO_PLAYBACK_FILE_MODE").is_some();
    let preview_directory = tempfile::tempdir().unwrap();
    let original_mode = std::env::var_os("CLIPSTUDIO_PLAYBACK_ORIGINAL").is_some();
    let urls: Vec<_> = paths
        .iter()
        .enumerate()
        .map(|(index, original)| {
            let preview = if file_mode || original_mode {
                original.clone()
            } else {
                clip_studio_lib::media::raw_video::preview_source(
                    original,
                    &preview_directory.path().join(index.to_string()),
                )
                .unwrap()
            };
            if !file_mode && !original_mode && index % 2 == 1 {
                assert_ne!(&preview, original);
                assert_eq!(preview.extension().unwrap(), "mp4");
            }
            if !file_mode && !original_mode && index % 2 == 0 {
                assert_eq!(&preview, original, "Ordinary video keeps its source");
            }
            let p = &preview;
            if file_mode {
                return reqwest::Url::from_file_path(p.canonicalize().unwrap())
                    .unwrap()
                    .to_string();
            }
            registry
                .issue_inline(
                    p.clone(),
                    p.file_name().unwrap().to_str().unwrap().into(),
                    if p.extension().unwrap().eq_ignore_ascii_case("mp4") {
                        "video/mp4"
                    } else {
                        "video/quicktime"
                    }
                    .into(),
                    &base,
                    3600,
                )
                .unwrap()
                .url
        })
        .collect();
    let html = format!(
        r#"<html><body><script>
const urls={}; const results=[]; let i=0;
window.run=async()=>{{
if(i===urls.length){{window.webkit.messageHandlers.result.postMessage(results);return;}}
const v=document.createElement('video');v.muted=false;v.controls=true;v.style.width='600px';v.style.height='320px';document.body.append(v);v.src=urls[i];
const events=[];for(const name of ['waiting','stalled','pause','emptied','durationchange','ended'])v.addEventListener(name,()=>events.push({{name,t:v.currentTime,d:Number.isFinite(v.duration)?v.duration:null,state:v.readyState,muted:v.muted,visibility:document.visibilityState}}));
try{{await Promise.race([v.play(),new Promise((_,r)=>setTimeout(()=>r(Error('play timeout')),7000))]);const start=v.currentTime;await new Promise(r=>setTimeout(r,i%2?6000:1200));const continuous=v.currentTime-start;const duration=v.duration;v.currentTime=Number.isFinite(duration)?Math.max(0,duration-1):5;await new Promise(r=>setTimeout(r,1600));results.push({{i,width:v.videoWidth,time:v.currentTime,duration,continuous,visibility:document.visibilityState,seekable:Array.from({{length:v.seekable.length}},(_,j)=>[v.seekable.start(j),v.seekable.end(j)]),buffered:Array.from({{length:v.buffered.length}},(_,j)=>[v.buffered.start(j),v.buffered.end(j)]),ended:v.ended,paused:v.paused,events,error:v.error?.code??null}})}}catch(e){{results.push({{i,width:v.videoWidth,error:v.error?.code??null,message:String(e)}})}}
v.pause();v.removeAttribute('src');v.load();v.remove();i++;
window.webkit.messageHandlers.result.postMessage({{progress:results[results.length-1],next:true}});
}};</script></body></html>"#,
        serde_json::to_string(&urls).unwrap()
    );
    let page = tempfile::Builder::new().suffix(".html").tempfile().unwrap();
    std::fs::write(page.path(), &html).unwrap();
    let probe_url = if file_mode {
        reqwest::Url::from_file_path(page.path())
            .unwrap()
            .to_string()
    } else {
        format!("{base}/probe")
    };
    let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let requests = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let f = failures.clone();
    let n = requests.clone();
    let trace = Arc::new(std::sync::Mutex::new(Vec::new()));
    let trace_out = trace.clone();
    let app = router(registry)
        .route(
            "/probe",
            get(move || {
                let html = html.clone();
                async move { axum::response::Html(html) }
            }),
        )
        .layer(axum::middleware::from_fn(
            move |request: axum::extract::Request, next: axum::middleware::Next| {
                let f = f.clone();
                let n = n.clone();
                let trace = trace.clone();
                async move {
                    trace.lock().unwrap().push(format!(
                        "{} {:?}",
                        request.uri().path(),
                        request.headers().get("range")
                    ));
                    let response = next.run(request).await;
                    n.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    if response.status() == StatusCode::TOO_MANY_REQUESTS {
                        f.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    response
                }
            },
        ));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let output = tokio::task::spawn_blocking(move || {
        std::process::Command::new(probe)
            .arg(probe_url)
            .output()
            .unwrap()
    })
    .await
    .unwrap();
    server.abort();
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let text = String::from_utf8_lossy(&output.stdout);
    eprintln!(
        "WebKit: {text}; requests={}; HTTP429={}",
        requests.load(std::sync::atomic::Ordering::Relaxed),
        failures.load(std::sync::atomic::Ordering::Relaxed)
    );
    if let Some(path) = std::env::var_os("CLIPSTUDIO_PLAYBACK_TRACE") {
        std::fs::write(path, trace_out.lock().unwrap().join("\n")).unwrap();
    }
    let results: Vec<serde_json::Value> = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(results.len(), 5);
    assert_eq!(failures.load(std::sync::atomic::Ordering::Relaxed), 0);
    for result in results {
        assert!(
            result["width"].as_u64().unwrap_or(0) > 0
                && result["time"].as_f64().unwrap_or(0.) > 5.
                && result["error"].is_null()
                && result["visibility"] == "visible"
                && result["duration"]
                    .as_f64()
                    .is_some_and(|duration| duration.is_finite() && duration > 0.)
                && result["continuous"].as_f64().unwrap_or(0.)
                    > if result["i"].as_u64().unwrap_or(0) % 2 == 1 {
                        4.
                    } else {
                        0.5
                    },
            "{result}"
        );
    }
}

#[tokio::test]
async fn playback_seek_connections_are_independent_of_mcp_download_limits() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("video.mp4");
    std::fs::write(&path, b"0123456789").unwrap();
    for (registry, count) in [
        (DownloadRegistry::for_local_playback(), 12),
        (DownloadRegistry::default(), 4),
    ] {
        let registry = Arc::new(registry);
        let grant = registry
            .issue_inline(
                path.clone(),
                "video.mp4".into(),
                "video/mp4".into(),
                "http://localhost",
                3600,
            )
            .unwrap();
        let app = router(registry);
        let mut active = Vec::new();
        for _ in 0..count {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(&grant.url)
                        .header("range", "bytes=0-1")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            active.push(response);
        }
        if count == 4 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(&grant.url)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        }
        drop(active);
        let response = app
            .oneshot(
                Request::builder()
                    .uri(format!("{}bad", grant.url))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "Local playback must still validate capability tokens"
        );
    }
}

#[tokio::test]
async fn local_playback_ranges_clamp_to_the_file_and_head_returns_no_body() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("video.mp4");
    std::fs::write(&path, b"0123456789").unwrap();
    let registry = Arc::new(DownloadRegistry::for_local_playback());
    let grant = registry
        .issue_inline(
            path,
            "字幕 video.mp4".into(),
            "video/mp4".into(),
            "http://localhost",
            3600,
        )
        .unwrap();
    let app = router(registry);
    for (range, expected, content_range) in [
        ("bytes=8-999", "89", "bytes 8-9/10"),
        ("bytes=-100", "0123456789", "bytes 0-9/10"),
        ("bytes=9-", "9", "bytes 9-9/10"),
    ] {
        for method in ["GET", "HEAD"] {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(method)
                        .uri(&grant.url)
                        .header("range", range)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
            assert_eq!(response.headers()["content-range"], content_range);
            assert_eq!(
                response.headers()["content-length"],
                expected.len().to_string()
            );
            assert_eq!(response.headers()["cache-control"], "private, no-store");
            let bytes = axum::body::to_bytes(response.into_body(), 10)
                .await
                .unwrap();
            assert_eq!(
                bytes.as_ref(),
                if method == "HEAD" {
                    b""
                } else {
                    expected.as_bytes()
                }
            );
        }
    }
}

#[tokio::test]
async fn invalid_ranges_do_not_poison_a_later_valid_playback_request() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("video.mp4");
    std::fs::write(&path, b"0123456789").unwrap();
    let registry = Arc::new(DownloadRegistry::for_local_playback());
    let grant = registry
        .issue_inline(
            path,
            "video.mp4".into(),
            "video/mp4".into(),
            "http://localhost",
            3600,
        )
        .unwrap();
    let app = router(registry);
    for range in [
        "bytes=10-",
        "bytes=5-4",
        "bytes=-0",
        "bytes=0-1,4-5",
        "bytes=18446744073709551616-",
        "items=0-1",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(&grant.url)
                    .header("range", range)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::RANGE_NOT_SATISFIABLE,
            "{range}"
        );
        assert_eq!(response.headers()["content-range"], "bytes */10");
        assert!(!axum::body::to_bytes(response.into_body(), 1024)
            .await
            .unwrap()
            .windows(10)
            .any(|v| v == b"0123456789"));
    }
    let response = app
        .oneshot(
            Request::builder()
                .uri(&grant.url)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        axum::body::to_bytes(response.into_body(), 10)
            .await
            .unwrap()
            .as_ref(),
        b"0123456789"
    );
}
#[cfg(target_os = "macos")]
#[path = "support/resources.rs"]
mod test_resources;
