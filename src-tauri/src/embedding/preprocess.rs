//! Gemma 4 patch layout, following Transformers.js 4.3.2 (Apache-2.0).
use anyhow::{bail, Result};
use image::{imageops::FilterType, RgbImage};

pub struct Patches {
    pub pixels: Vec<f32>,
    pub positions: Vec<i64>,
    pub soft_tokens: usize,
    pub max_patches: usize,
}

pub fn target_size(width: u32, height: u32, budget: usize) -> Result<(u32, u32)> {
    if width == 0 || height == 0 || ![70, 140, 280, 560, 1120].contains(&budget) {
        bail!("Invalid image dimensions or visual token budget");
    }
    let max_patches = budget * 9;
    let factor = ((max_patches * 256) as f64 / (width as f64 * height as f64)).sqrt();
    let mut h = ((factor * height as f64 / 48.0).floor() as u32) * 48;
    let mut w = ((factor * width as f64 / 48.0).floor() as u32) * 48;
    if h == 0 && w == 0 {
        bail!("Unsupported image aspect ratio");
    }
    if h == 0 {
        h = 48;
        w = (width / height * 48).min(budget as u32 * 48);
    }
    if w == 0 {
        w = 48;
        h = (height / width * 48).min(budget as u32 * 48);
    }
    Ok((w, h))
}

pub fn patchify(image: &RgbImage, budget: usize) -> Result<Patches> {
    let (width, height) = target_size(image.width(), image.height(), budget)?;
    let resized = image::imageops::resize(image, width, height, FilterType::CatmullRom);
    patchify_resized(&resized, budget)
}

pub fn patchify_resized(resized: &RgbImage, budget: usize) -> Result<Patches> {
    let (width, height) = resized.dimensions();
    if width == 0
        || height == 0
        || width % 48 != 0
        || height % 48 != 0
        || width as usize * height as usize / 256 > budget * 9
    {
        bail!("Invalid resized video dimensions");
    }
    let max_patches = budget * 9;
    let mut pixels = vec![0.0; max_patches * 768];
    let mut positions = vec![-1; max_patches * 2];
    let mut patch = 0;
    for row in 0..height / 16 {
        for col in 0..width / 16 {
            positions[patch * 2] = col as i64;
            positions[patch * 2 + 1] = row as i64;
            let mut offset = patch * 768;
            for dy in 0..16 {
                for dx in 0..16 {
                    for channel in resized.get_pixel(col * 16 + dx, row * 16 + dy).0 {
                        pixels[offset] = channel as f32 / 255.0;
                        offset += 1;
                    }
                }
            }
            patch += 1;
        }
    }
    Ok(Patches {
        pixels,
        positions,
        soft_tokens: patch / 9,
        max_patches,
    })
}

pub fn media_tokens(counts: &[usize], video: bool) -> Vec<i64> {
    let mut ids = vec![2]; // BOS; media do not receive a text retrieval prefix.
    for count in counts {
        ids.push(255999); // <|image>
        ids.extend(std::iter::repeat_n(
            if video { 258884 } else { 258880 },
            *count,
        ));
        ids.push(258882); // <image|>
    }
    ids.push(1); // EOS
    ids
}
