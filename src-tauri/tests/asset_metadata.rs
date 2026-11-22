use clip_studio_lib::media::{self, metadata};

fn ffmpeg() -> bool {
    if std::env::var_os("CLIPSTUDIO_FFMPEG").is_some() && media::ffmpeg_available() {
        return true;
    }
    assert_ne!(
        std::env::var("CLIPSTUDIO_CI_REQUIRE_RESOURCES").as_deref(),
        Ok("1"),
        "Strict metadata tests require CLIPSTUDIO_FFMPEG"
    );
    eprintln!("SKIPPED real media metadata: set CLIPSTUDIO_FFMPEG");
    false
}

#[test]
fn reads_real_video_audio_tags_and_file_times_without_changing_the_file() {
    if !ffmpeg() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("metadata.mp4");
    let output = media::ffmpeg_command()
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=160x90:r=25:d=0.5",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=0.5",
            "-c:v",
            "mpeg4",
            "-c:a",
            "aac",
            "-metadata",
            "title=演示: metadata",
            "-metadata:s:a:0",
            "language=eng",
            "-shortest",
            "-y",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let bytes = std::fs::read(&path).unwrap();
    let info = metadata::read(&path).unwrap();
    assert!(info.format.as_deref().unwrap().contains("mp4"));
    assert!(info.modified_at.is_some());
    assert!(info
        .tags
        .iter()
        .any(|tag| tag.name == "title" && tag.value == "演示: metadata"));
    let video = info.streams.iter().find(|s| s.kind == "video").unwrap();
    assert_eq!(video.codec, "mpeg4");
    assert_eq!((video.width, video.height), (Some(160), Some(90)));
    assert_eq!(video.frame_rate, Some(25.));
    let audio = info.streams.iter().find(|s| s.kind == "audio").unwrap();
    assert_eq!(audio.codec, "aac");
    assert_eq!(audio.sample_rate, Some(48000));
    assert_eq!(audio.language.as_deref(), Some("eng"));
    let json = serde_json::to_value(&info).unwrap();
    assert!(json.get("modifiedAt").is_some());
    assert!(json["streams"][1].get("sampleRate").is_some());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    eprintln!(
        "EXECUTED real FFmpeg metadata, Unicode tags, audio/video fields and read-only source"
    );
}

#[test]
fn reads_a_real_png_without_requiring_a_video_or_audio_track() {
    if !ffmpeg() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("image.png");
    image::RgbImage::new(32, 16).save(&path).unwrap();
    let info = metadata::read(&path).unwrap();
    assert_eq!(info.streams.len(), 1);
    assert_eq!(info.streams[0].codec, "png");
    assert_eq!(
        (info.streams[0].width, info.streams[0].height),
        (Some(32), Some(16))
    );
    eprintln!("EXECUTED real image metadata");
}

#[test]
fn missing_and_invalid_files_are_recoverable_errors() {
    let temp = tempfile::tempdir().unwrap();
    assert!(metadata::read(&temp.path().join("missing.png")).is_err());
    if !ffmpeg() {
        return;
    }
    let invalid = temp.path().join("invalid.mp4");
    std::fs::write(&invalid, b"not a media file").unwrap();
    assert_eq!(
        metadata::read(&invalid).unwrap_err().code(),
        "asset_metadata_failed"
    );
}

#[test]
fn reads_photo_exif_gps_lens_and_exposure_without_changing_source() {
    if !ffmpeg() {
        return;
    }
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("exif.png");
    let bytes = include_bytes!("../../tests/fixtures/exif-demo.png");
    std::fs::write(&path, bytes).unwrap();
    let json = serde_json::to_value(metadata::read(&path).unwrap()).unwrap();
    let tags = json["exif"]
        .as_array()
        .expect("EXIF must be returned separately");
    for (name, expected) in [
        ("Make", "ClipSeek Demo"),
        ("Model", "Demo Camera"),
        ("LensModel", "Demo 50mm F2.8"),
        ("FocalLength", "50 mm"),
        ("ExposureTime", "1/125 s"),
        ("FNumber", "f/2.8"),
        ("PhotographicSensitivity", "200"),
        ("GPSLatitude", "N"),
        ("GPSLongitude", "E"),
        ("GPSAltitude", "12.5"),
    ] {
        let value = tags
            .iter()
            .find(|tag| tag["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}"));
        assert!(
            value["value"].as_str().unwrap().contains(expected),
            "{name}: {value}"
        );
    }
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}
