use super::*;
use std::io::Cursor;

const TIFF: &[u8] = include_bytes!("../../../tests/fixtures/exif-demo.tiff");
const PNG: &[u8] = include_bytes!("../../../tests/fixtures/exif-demo.png");

fn tags(bytes: &[u8]) -> Vec<MetadataTag> {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("photo");
    std::fs::write(&path, bytes).unwrap();
    let tags = read(&path);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    tags
}

fn value<'a>(tags: &'a [MetadataTag], name: &str) -> &'a str {
    &tags
        .iter()
        .find(|tag| tag.name == name)
        .unwrap_or_else(|| panic!("Missing {name}"))
        .value
}

#[test]
fn reads_tiff_png_and_jpeg_by_content_with_units_and_gps_references() {
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut jpeg)
        .encode(&[100; 12], 2, 2, image::ExtendedColorType::Rgb8)
        .unwrap();
    let mut with_exif = vec![0xff, 0xd8, 0xff, 0xe1];
    with_exif.extend_from_slice(&((TIFF.len() + 8) as u16).to_be_bytes());
    with_exif.extend_from_slice(b"Exif\0\0");
    with_exif.extend_from_slice(TIFF);
    with_exif.extend_from_slice(&jpeg[2..]);
    for bytes in [TIFF, PNG, with_exif.as_slice()] {
        let fields = tags(bytes);
        assert_eq!(value(&fields, "Make"), "ClipSeek Demo");
        assert_eq!(value(&fields, "LensModel"), "Demo 50mm F2.8");
        assert_eq!(value(&fields, "FocalLength"), "50 mm");
        assert_eq!(value(&fields, "FNumber"), "f/2.8");
        assert_eq!(value(&fields, "ExposureTime"), "1/125 s");
        assert_eq!(value(&fields, "PhotographicSensitivity"), "200");
        assert_eq!(value(&fields, "ExifVersion"), "2.32");
        assert_eq!(value(&fields, "MakerNote"), "8 bytes");
        assert_eq!(value(&fields, "DateTimeOriginal"), "2026:11:07 10:30:00");
        assert_eq!(value(&fields, "OffsetTimeOriginal"), "+08:00");
        assert!(value(&fields, "GPSLatitude").contains("N"));
        assert!(value(&fields, "GPSLongitude").contains("E"));
        assert!(value(&fields, "GPSAltitude").contains("12.5"));
    }
}

#[test]
fn missing_absent_or_broken_exif_does_not_fail_the_request() {
    assert!(read(Path::new("/nonexistent/clipseek-photo.jpg")).is_empty());
    assert!(tags(b"not a photo").is_empty());
    assert!(tags(b"II\x2a\0\xff\xff\xff\xff").is_empty());
    let mut plain = Vec::new();
    image::codecs::jpeg::JpegEncoder::new(&mut plain)
        .encode(&[0; 12], 2, 2, image::ExtendedColorType::Rgb8)
        .unwrap();
    assert!(tags(&plain).is_empty());
}

#[test]
fn partial_exif_keeps_valid_fields_and_thumbnail_fields_stay_distinct() {
    let mut bytes = TIFF.to_vec();
    // Invalid Orientation type; other fields remain usable in tolerant mode.
    bytes[8 + 2 + 3 * 12 + 2..8 + 2 + 3 * 12 + 4].copy_from_slice(&999_u16.to_le_bytes());
    assert_eq!(value(&tags(&bytes), "LensModel"), "Demo 50mm F2.8");
    let mut bytes = TIFF.to_vec();
    let offset = bytes.len() as u32;
    bytes[8 + 2 + 6 * 12..8 + 2 + 6 * 12 + 4].copy_from_slice(&offset.to_le_bytes());
    bytes.extend_from_slice(&1_u16.to_le_bytes());
    bytes.extend_from_slice(&0x010f_u16.to_le_bytes());
    bytes.extend_from_slice(&2_u16.to_le_bytes());
    bytes.extend_from_slice(&4_u32.to_le_bytes());
    bytes.extend_from_slice(b"Sub\0");
    bytes.extend_from_slice(&0_u32.to_le_bytes());
    let fields = tags(&bytes);
    assert_eq!(value(&fields, "Make"), "ClipSeek Demo");
    assert_eq!(value(&fields, "IFD1.Make"), "Sub");
}

#[test]
fn budget_counts_reads_even_after_seek() {
    let mut reader = BudgetReader {
        source: Cursor::new([0; 32]),
        remaining: 4,
    };
    let mut buffer = [0; 3];
    assert_eq!(reader.read(&mut buffer).unwrap(), 3);
    reader.seek(SeekFrom::Start(0)).unwrap();
    assert_eq!(reader.read(&mut buffer).unwrap(), 1);
    assert!(reader.read(&mut buffer).is_err());
}
