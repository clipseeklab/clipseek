use super::*;

#[test]
fn parses_video_audio_and_scoped_tags_without_losing_colons_or_unicode() {
    let summary = r#"Input #0, mov,mp4,m4a, from '/tmp/a,b.mp4':
  Metadata:
    title           : 演示视频
    creation_time   : 2026-11-06T12:34:56.000000Z
    description     : first: value
                    : 第二行
  Duration: 00:01:02.50, start: 0.000000, bitrate: 1200 kb/s
  Stream #0:0[0x1](und): Video: h264 (High) (avc1 / 0x31637661), yuv420p(tv, bt709, progressive), 1920x1080 [SAR 1:1 DAR 16:9], 1000 kb/s, 29.97 fps, 30 tbr
    Metadata:
      handler_name    : VideoHandler
    Side data:
      displaymatrix: rotation of 90.00 degrees
  Stream #0:1[0x2](eng): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 128 kb/s (default)
    Metadata:
      title           : English: Main
At least one output file must be specified
"#;
    let data = parse(summary).unwrap();
    assert_eq!(data.format.as_deref(), Some("mov,mp4,m4a"));
    assert_eq!(data.bit_rate, Some(1_200_000));
    assert_eq!(data.tags.len(), 3);
    assert_eq!(data.tags[1].value, "2026-11-06T12:34:56.000000Z");
    assert_eq!(data.tags[2].value, "first: value\n第二行");
    let video = &data.streams[0];
    assert_eq!(video.codec, "h264");
    assert_eq!(video.profile.as_deref(), Some("High"));
    assert_eq!((video.width, video.height), (Some(1920), Some(1080)));
    assert_eq!(video.frame_rate, Some(29.97));
    assert_eq!(video.bit_rate, Some(1_000_000));
    assert_eq!(
        video.pixel_format.as_deref(),
        Some("yuv420p(tv, bt709, progressive)")
    );
    assert_eq!(video.tags.len(), 2);
    assert_eq!(video.tags[1].name, "displaymatrix");
    assert_eq!(video.tags[1].value, "rotation of 90.00 degrees");
    let audio = &data.streams[1];
    assert_eq!(audio.index, 1);
    assert_eq!(audio.kind, "audio");
    assert_eq!(audio.language.as_deref(), Some("eng"));
    assert_eq!(audio.sample_rate, Some(48000));
    assert_eq!(audio.channel_layout.as_deref(), Some("stereo"));
    assert_eq!(audio.sample_format.as_deref(), Some("fltp"));
    assert_eq!(audio.tags[0].value, "English: Main");
}

#[test]
fn unknown_values_and_codec_tags_do_not_become_fake_dimensions_or_rates() {
    let data = parse("Input #0, image2, from 'a':\n  Duration: N/A, bitrate: N/A\n  Stream #0:0: Video: av1 (av01 / 0x31307661), yuv420p, 1280x720, NaN fps\n").unwrap();
    assert_eq!(data.bit_rate, None);
    assert_eq!(data.streams[0].profile, None);
    assert_eq!(data.streams[0].language, None);
    assert_eq!(data.streams[0].frame_rate, None);
    assert_eq!(data.streams[0].width, Some(1280));
    assert!(parse("missing file: Invalid data").is_err());
    assert_eq!(bit_rate("-1 kb/s"), None);
    assert_eq!(bit_rate("inf Mb/s"), None);
    assert_eq!(bit_rate("1.5 Mb/s"), Some(1_500_000));
}

#[test]
fn limits_tag_count_and_unicode_values_and_keeps_stream_ownership() {
    let mut summary = "Input #0, matroska, from 'a':\n  Metadata:\n".to_owned();
    for i in 0..300 {
        summary.push_str(&format!("    tag{i}: {}\n", "中".repeat(5000)));
    }
    let data = parse(&summary).unwrap();
    assert_eq!(data.tags.len(), MAX_TAGS);
    assert_eq!(data.tags[0].value.chars().count(), 4096);
    let mut summary = "Input #0, matroska, from 'a':\n".to_owned();
    for i in 0..70 {
        summary.push_str(&format!("  Stream #0:{i}: Audio: aac, 48000 Hz, mono, fltp\n    Metadata:\n      name: stream{i}\n"));
    }
    let data = parse(&summary).unwrap();
    assert_eq!(data.streams.len(), 64);
    assert!(data.tags.is_empty());
}

#[cfg(unix)]
#[test]
fn probe_deadline_and_output_limit_terminate_the_child() {
    let mut command = Command::new("sh");
    command.args(["-c", "exec sleep 5"]);
    let started = Instant::now();
    assert!(input_summary(command, Duration::from_millis(30)).is_err());
    assert!(started.elapsed() < Duration::from_secs(2));
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "while :; do printf 'diagnostic diagnostic diagnostic\\n' >&2; done",
    ]);
    assert!(input_summary(command, Duration::from_secs(2)).is_err());
}
