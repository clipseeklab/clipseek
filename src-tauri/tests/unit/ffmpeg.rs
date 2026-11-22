use super::*;

#[test]
fn parses_ffmpeg_input_summary() {
    let summary = r#"
Input #0, mov,mp4, from 'sample.mp4':
  Duration: 00:01:02.50, start: 0.000000, bitrate: 1200 kb/s
  Stream #0:0: Video: h264 (High), yuv420p, 1920x1080, 29.97 fps, 30 tbr
"#;
    let info = parse_media_info(summary);
    assert_eq!(info.width, Some(1920));
    assert_eq!(info.height, Some(1080));
    assert_eq!(info.duration_ms, Some(62_500));
    assert_eq!(info.fps, Some(29.97));
}

#[test]
fn ignores_codec_tag_that_looks_like_resolution() {
    let summary = r#"
  Duration: 00:24:17.00, start: 0.000000, bitrate: 1460 kb/s
  Stream #0:0: Video: av1 (Main) (av01 / 0x31307661), yuv420p, 1920x1080, 30 fps
"#;
    let info = parse_media_info(summary);
    assert_eq!((info.width, info.height), (Some(1920), Some(1080)));
}

#[test]
fn detects_silent_webkit_hev1_failure_before_playback() {
    let hev1 = "  Stream #0:0: Video: hevc (Main) (hev1 / 0x31766568), yuv420p, 1920x1080, 24 fps";
    let hvc1 =
        "  Stream #0:0: Video: hevc (Main 10) (hvc1 / 0x31637668), yuv420p10le, 1920x1080, 24 fps";
    assert!(summary_needs_webkit_compatibility(hev1));
    assert!(!summary_needs_webkit_compatibility(hvc1));
}
