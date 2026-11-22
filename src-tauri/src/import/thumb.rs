//! WebP thumbnail encoding through the image crate.

use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

use image::codecs::webp::WebPEncoder;
use image::imageops::FilterType;
use image::{ColorType, DynamicImage, GenericImageView, ImageDecoder, ImageEncoder};

use crate::error::{AppError, AppResult};
use crate::media::ffmpeg::RgbFrame;

/// Maximum thumbnail edge length.
pub const THUMB_MAX: u32 = 512;

/// Bound SVG rasterization dimensions to prevent memory exhaustion.
const SVG_MAX_DIMENSION: u32 = 16_384;
const SVG_MAX_PIXELS: u64 = 64 * 1024 * 1024;

fn extension(src: &Path) -> String {
    src.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default()
}

/// Formats that require rasterized import previews rather than direct WebView/image decoding.
pub fn needs_rasterized_preview(src: &Path) -> bool {
    matches!(extension(src).as_str(), "svg" | "heic" | "heif")
}

/// Encode an RGB frame to a WebP file.
pub fn encode_rgb_frame(path: &Path, frame: &RgbFrame) -> AppResult<()> {
    crate::util::replace_file(path, |temporary| {
        let mut writer = BufWriter::new(File::create(temporary)?);
        WebPEncoder::new_lossless(&mut writer)
            .write_image(
                &frame.data,
                frame.width,
                frame.height,
                ColorType::Rgb8.into(),
            )
            .map_err(|e| AppError::Msg(format!("webp encode failed: {e}")))?;
        writer.flush()?;
        Ok(())
    })
}

/// Detect image format from content so misleading extensions do not cause decoding failures.
pub fn open_image_smart(src: &Path) -> AppResult<DynamicImage> {
    match extension(src).as_str() {
        "svg" => return open_svg(src),
        "heic" | "heif" => return open_heif(src),
        _ => {}
    }
    open_raster_with_orientation(src)
}

/// Decode raster images and apply EXIF orientation; ordinary image decoding does not rotate
/// pixels automatically, including JPEGs produced from HEIC.
fn open_raster_with_orientation(src: &Path) -> AppResult<DynamicImage> {
    let reader = image::ImageReader::open(src)
        .map_err(|e| AppError::Msg(format!("image open failed: {e}")))?
        .with_guessed_format()
        .map_err(|e| AppError::Msg(format!("image format detection failed: {e}")))?;
    let mut decoder = reader
        .into_decoder()
        .map_err(|e| AppError::Msg(format!("image decoder init failed: {e}")))?;
    let orientation = decoder
        .orientation()
        .map_err(|e| AppError::Msg(format!("image orientation read failed: {e}")))?;
    let mut image = DynamicImage::from_decoder(decoder)
        .map_err(|e| AppError::Msg(format!("image decode failed: {e}")))?;
    image.apply_orientation(orientation);
    Ok(image)
}

#[cfg(target_os = "windows")]
fn open_heif(src: &Path) -> AppResult<DynamicImage> {
    open_heif_windows(src)
}

#[cfg(not(target_os = "windows"))]
fn open_heif(src: &Path) -> AppResult<DynamicImage> {
    if !crate::media::sips::available() {
        return Err(AppError::Msg(
            "HEIC/HEIF decoding is unavailable on this system (macOS sips required)".into(),
        ));
    }
    let temp = tempfile::Builder::new()
        .prefix("clip-studio-heif-")
        .suffix(".jpg")
        .tempfile()
        .map_err(|e| AppError::Msg(format!("HEIC temp file creation failed: {e}")))?;
    crate::media::sips::heic_to_jpeg(src, temp.path())?;
    open_raster_with_orientation(temp.path())
        .map_err(|e| AppError::Msg(format!("HEIC decode failed: {e}")))
}

#[cfg(target_os = "windows")]
fn open_heif_windows(src: &Path) -> AppResult<DynamicImage> {
    super::heif_windows::decode(src)
}

/// Return full-size PNG only when the WebView cannot display HEIC/HEIF directly.
pub fn full_heif_preview_png(src: &Path) -> AppResult<Vec<u8>> {
    if !matches!(extension(src).as_str(), "heic" | "heif") {
        return Err(AppError::Msg(
            "full HEIC preview requires a HEIC/HEIF file".into(),
        ));
    }
    {
        let image = open_heif(src)?;
        let mut output = std::io::Cursor::new(Vec::new());
        image
            .write_to(&mut output, image::ImageFormat::Png)
            .map_err(|error| AppError::Msg(format!("HEIC/HEIF preview encode failed: {error}")))?;
        let bytes = output.into_inner();
        #[cfg(target_os = "windows")]
        if bytes.len() > 256 * 1024 * 1024 {
            return Err(AppError::Msg(
                "HEIC/HEIF preview exceeds the safety limit".into(),
            ));
        }
        Ok(bytes)
    }
}

fn open_svg(src: &Path) -> AppResult<DynamicImage> {
    let data = std::fs::read(src)?;
    let mut options = resvg::usvg::Options::default();
    // Prevent SVG access to local or network resources; embedded data URLs remain allowed.
    options.image_href_resolver.resolve_string = Box::new(|_, _| None);
    let tree = resvg::usvg::Tree::from_data(&data, &options)
        .map_err(|e| AppError::Msg(format!("SVG parse failed: {e}")))?;
    let size = tree.size().to_int_size();
    let missing_root_size = roxmltree::Document::parse(std::str::from_utf8(&data).unwrap_or(""))
        .ok()
        .map(|doc| {
            let root = doc.root_element();
            root.attribute("width").is_none()
                && root.attribute("height").is_none()
                && root.attribute("viewBox").is_none()
        })
        .unwrap_or(false);
    let (width, height) = if missing_root_size {
        (100, 100)
    } else {
        (size.width(), size.height())
    };
    if width > SVG_MAX_DIMENSION
        || height > SVG_MAX_DIMENSION
        || u64::from(width) * u64::from(height) > SVG_MAX_PIXELS
    {
        return Err(AppError::Msg(format!(
            "SVG dimensions too large: {width}x{height} (maximum {SVG_MAX_DIMENSION}px / {SVG_MAX_PIXELS} pixels)"
        )));
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| AppError::Msg(format!("SVG canvas creation failed: {width}x{height}")))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::default(),
        &mut pixmap.as_mut(),
    );
    let rgba = image::RgbaImage::from_raw(width, height, pixmap.take_demultiplied())
        .ok_or_else(|| AppError::Msg("invalid SVG pixel data".into()))?;
    Ok(DynamicImage::ImageRgba8(rgba))
}

/// Read dimensions from headers without decoding full pixels.
pub fn image_dimensions(src: &Path) -> AppResult<(u32, u32)> {
    if needs_rasterized_preview(src) {
        return Ok(open_image_smart(src)?.dimensions());
    }
    image::ImageReader::open(src)
        .map_err(|e| AppError::Msg(format!("image open failed: {e}")))?
        .with_guessed_format()
        .map_err(|e| AppError::Msg(format!("image format detection failed: {e}")))?
        .into_dimensions()
        .map_err(|e| AppError::Msg(format!("image size read failed: {e}")))
}

/// Generate a thumbnail only above THUMB_MAX; smaller images can use the original file.
pub fn image_thumb(src: &Path) -> AppResult<ImageProbe> {
    let img = open_image_smart(src)?;
    let (w, h) = img.dimensions();
    let max = w.max(h);
    let thumb_img = if max > THUMB_MAX {
        Some(img.resize(THUMB_MAX, THUMB_MAX, FilterType::CatmullRom))
    } else {
        None
    };
    Ok(ImageProbe {
        width: w as i64,
        height: h as i64,
        thumb: thumb_img,
    })
}

/// Static-image probe result; a present thumbnail must be persisted.
pub struct ImageProbe {
    pub width: i64,
    pub height: i64,
    pub thumb: Option<DynamicImage>,
}

#[cfg(test)]
#[path = "../../tests/unit/import_thumb.rs"]
mod tests;

/// Write a DynamicImage as lossless WebP, including specialized SVG/HEIC decode results.
pub fn encode_dynamic(path: &Path, img: &DynamicImage) -> AppResult<()> {
    let rgb = preview_rgb(img);
    crate::util::replace_file(path, |temporary| {
        let mut writer = BufWriter::new(File::create(temporary)?);
        WebPEncoder::new_lossless(&mut writer)
            .write_image(
                rgb.as_raw(),
                rgb.width(),
                rgb.height(),
                ColorType::Rgb8.into(),
            )
            .map_err(|error| AppError::Msg(format!("webp encode failed: {error}")))?;
        writer.flush()?;
        Ok(())
    })
}

/// Composite colors onto white before analysis; discarding alpha would misclassify
/// transparent black pixels.
pub fn preview_rgb(img: &DynamicImage) -> image::RgbImage {
    let rgba = img.to_rgba8();
    image::RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let alpha = u16::from(a);
        let blend =
            |channel: u8| ((u16::from(channel) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
        image::Rgb([blend(r), blend(g), blend(b)])
    })
}
