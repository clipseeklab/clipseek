use super::*;
use std::io::Write;

fn svg_file(contents: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new().suffix(".svg").tempfile().unwrap();
    file.write_all(contents.as_bytes()).unwrap();
    file
}

#[test]
fn rasterizes_svg_with_fixed_size_and_transparency() {
    let file = svg_file(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect x="10" width="30" height="20" fill="red"/></svg>"#,
    );
    let img = open_image_smart(file.path()).unwrap().to_rgba8();
    assert_eq!(img.dimensions(), (40, 20));
    assert_eq!(img.get_pixel(0, 0).0[3], 0);
    assert_eq!(img.get_pixel(20, 10).0, [255, 0, 0, 255]);
}

#[test]
fn rasterizes_viewbox_only_svg_using_default_viewport() {
    let file = svg_file(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 100"><rect width="200" height="100" fill="blue"/></svg>"#,
    );
    assert_eq!(image_dimensions(file.path()).unwrap(), (200, 100));
}

#[test]
fn uses_default_canvas_when_svg_has_no_size_or_viewbox() {
    let file = svg_file(
        r#"<svg xmlns="http://www.w3.org/2000/svg"><circle cx="50" cy="50" r="20"/></svg>"#,
    );
    assert_eq!(image_dimensions(file.path()).unwrap(), (100, 100));
}

#[test]
fn rejects_invalid_and_oversized_svg() {
    let invalid = svg_file("not svg");
    assert!(open_image_smart(invalid.path()).is_err());
    let huge =
        svg_file(r#"<svg xmlns="http://www.w3.org/2000/svg" width="20000" height="20000"/>"#);
    assert!(open_image_smart(huge.path())
        .unwrap_err()
        .to_string()
        .contains("dimensions too large"));
}

#[test]
fn blocks_external_svg_images() {
    let file = svg_file(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image href="/etc/passwd" width="10" height="10"/></svg>"#,
    );
    assert!(open_image_smart(file.path()).is_ok());
}

#[test]
fn composites_transparent_preview_pixels_over_white() {
    let img = DynamicImage::ImageRgba8(
        image::RgbaImage::from_raw(2, 1, vec![0, 0, 0, 0, 255, 0, 0, 128]).unwrap(),
    );
    let rgb = preview_rgb(&img);
    assert_eq!(rgb.get_pixel(0, 0).0, [255, 255, 255]);
    assert_eq!(rgb.get_pixel(1, 0).0, [255, 127, 127]);
}
