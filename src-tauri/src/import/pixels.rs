//! Frame quality and dominant-color analysis over RGB24 buffers in Rust.

/// Box-sample a grayscale thumbnail.
fn to_grayscale_downsampled(
    width: u32,
    height: u32,
    data: &[u8],
    target_w: u32,
) -> (u32, Vec<f32>) {
    if width == 0 || height == 0 || data.len() < (width * height * 3) as usize {
        return (0, Vec::new());
    }
    let scale = width as f32 / target_w as f32;
    let target_h = ((height as f32 / scale).ceil() as u32).max(1);
    let mut gray = Vec::with_capacity((target_w * target_h) as usize);
    for y in 0..target_h {
        let sy = (((y as f32 + 0.5) * scale).floor() as u32).min(height - 1);
        for x in 0..target_w {
            let sx = (((x as f32 + 0.5) * scale).floor() as u32).min(width - 1);
            let i = ((sy * width + sx) * 3) as usize;
            let r = data[i] as f32;
            let g = data[i + 1] as f32;
            let b = data[i + 2] as f32;
            gray.push(0.299 * r + 0.587 * g + 0.114 * b);
        }
    }
    (target_w, gray)
}

/// Laplacian variance; lower values are blurrier, with a historical threshold of 45.
pub fn blur_score(width: u32, height: u32, data: &[u8]) -> f32 {
    let (w, g) = to_grayscale_downsampled(width, height, data, 160);
    if w == 0 || g.len() < 4 {
        return 0.0;
    }
    let h = (g.len() / w as usize) as u32;
    let at = |x: u32, y: u32| g[(y * w + x) as usize];
    let mut sum = 0.0;
    let mut sum_sq = 0.0;
    let mut n = 0u32;
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            // Four-neighbor Laplacian operator.
            let lap = 4.0 * at(x, y) - at(x - 1, y) - at(x + 1, y) - at(x, y - 1) - at(x, y + 1);
            sum += lap;
            sum_sq += lap * lap;
            n += 1;
        }
    }
    if n == 0 {
        return 0.0;
    }
    let mean = sum / n as f32;
    sum_sq / n as f32 - mean * mean
}

/// Detect nearly empty frames using a small grayscale histogram, much cheaper than inference
/// or WebP encoding.
pub fn is_low_information(width: u32, height: u32, data: &[u8]) -> bool {
    let (_, gray) = to_grayscale_downsampled(width, height, data, 96);
    if gray.len() < 64 {
        return true;
    }

    let mut histogram = [0usize; 32];
    let mut sum = 0.0f32;
    let mut sum_sq = 0.0f32;
    for &value in &gray {
        let bin = ((value as usize) / 8).min(31);
        histogram[bin] += 1;
        sum += value;
        sum_sq += value * value;
    }
    let count = gray.len() as f32;
    let mean = sum / count;
    let variance = (sum_sq / count - mean * mean).max(0.0);
    let peak_ratio = histogram.iter().copied().max().unwrap_or(0) as f32 / count;
    let entropy = histogram.iter().fold(0.0f32, |acc, &n| {
        if n == 0 {
            acc
        } else {
            let p = n as f32 / count;
            acc - p * p.log2()
        }
    }) / 5.0; // A 32-bin histogram has maximum entropy 5; normalize to 0..1.

    peak_ratio >= 0.985 || (variance < 20.0 && entropy < 0.12)
}

/// A 64-bit perceptual dHash from a fixed 9x8 grid for adjacent-frame deduplication.
pub fn dhash(width: u32, height: u32, data: &[u8]) -> u64 {
    if width == 0 || height == 0 || data.len() < (width * height * 3) as usize {
        return 0;
    }
    // Always sample exactly 9x8 pixels. Aspect-preserving width scaling can produce fewer
    // than eight rows and collapse all hashes to zero.
    let gray = |x: u32, y: u32| {
        let sx = (((x as f32 + 0.5) * width as f32 / 9.0).floor() as u32).min(width - 1);
        let sy = (((y as f32 + 0.5) * height as f32 / 8.0).floor() as u32).min(height - 1);
        let i = ((sy * width + sx) * 3) as usize;
        0.299 * data[i] as f32 + 0.587 * data[i + 1] as f32 + 0.114 * data[i + 2] as f32
    };
    let mut hash: u64 = 0;
    let mut bit = 0;
    for y in 0..8 {
        for x in 0..8 {
            if gray(x + 1, y) > gray(x, y) {
                hash |= 1 << bit;
            }
            bit += 1;
        }
    }
    hash
}

/// Hamming distance.
pub fn hamming(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

/// Map hue to one of eight chromatic buckets.
fn hue_bin(h: f32) -> usize {
    match h {
        h if (345.0..=360.0).contains(&h) || (0.0..15.0).contains(&h) => 0, // red
        h if (15.0..40.0).contains(&h) => 1,                                // orange
        h if (40.0..70.0).contains(&h) => 2,                                // yellow
        h if (70.0..160.0).contains(&h) => 3,                               // green
        h if (160.0..200.0).contains(&h) => 4,                              // cyan
        h if (200.0..255.0).contains(&h) => 5,                              // blue
        h if (255.0..290.0).contains(&h) => 6,                              // purple
        _ => 7,                                                             // magenta 290~345
    }
}

const CHROMATIC: [&str; 8] = [
    "red", "orange", "yellow", "green", "cyan", "blue", "purple", "magenta",
];

#[derive(Debug, Clone, PartialEq)]
pub struct ColorShare {
    pub color: &'static str,
    pub ratio: f32,
}

const MIN_COLOR_RATIO: f32 = 0.10;
const MAX_COLORS: usize = 3;

/// Compute twelve dominant-color buckets using saturation-weighted hue, three achromatic
/// ranges, and a separate dark orange/yellow brown bucket.
pub fn dominant_color(width: u32, height: u32, data: &[u8]) -> &'static str {
    dominant_colors(width, height, data)
        .first()
        .map(|c| c.color)
        .unwrap_or("gray")
}

/// Return up to three colors by sampled-pixel share, ignoring shares below 10%. Keep the
/// largest bucket if none reaches the threshold.
pub fn dominant_colors(width: u32, height: u32, data: &[u8]) -> Vec<ColorShare> {
    if width == 0 || height == 0 || data.len() < (width * height * 3) as usize {
        return vec![ColorShare {
            color: "gray",
            ratio: 1.0,
        }];
    }
    // Choose a stride yielding roughly 40,000 sampled pixels.
    let stride = (((width * height) / 40_000).max(1)) as usize;
    let mut bins = [0f32; 9]; // Hue buckets 0..8, with 8 reserved for brown.
    let (mut black, mut white, mut gray) = (0f32, 0f32, 0f32);
    let px = (width * height) as usize;
    let mut i = 0usize;
    while i < px {
        let r = data[i * 3] as f32 / 255.0;
        let g = data[i * 3 + 1] as f32 / 255.0;
        let b = data[i * 3 + 2] as f32 / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let v = max;
        let d = max - min;
        let s = if max > 0.0 { d / max } else { 0.0 };

        if s < 0.18 || v < 0.09 {
            // Split achromatic pixels into three brightness ranges.
            if v < 0.18 {
                black += 1.0;
            } else if v > 0.82 {
                white += 1.0;
            } else {
                gray += 1.0;
            }
        } else {
            // HSV hue.
            let h = if (max - r).abs() < f32::EPSILON {
                60.0 * (((g - b) / d) % 6.0)
            } else if (max - g).abs() < f32::EPSILON {
                60.0 * ((b - r) / d + 2.0)
            } else {
                60.0 * ((r - g) / d + 4.0)
            };
            let h = if h < 0.0 { h + 360.0 } else { h };

            let bin = if (15.0..45.0).contains(&h) && (v < 0.62 || s < 0.45) {
                8 // Brown is a dark or desaturated orange/yellow hue.
            } else {
                hue_bin(h)
            };
            bins[bin] += 1.0;
        }
        i += stride;
    }

    let mut counts: Vec<(&'static str, f32)> = CHROMATIC
        .iter()
        .enumerate()
        .map(|(i, &name)| (name, bins[i]))
        .collect();
    counts.push(("brown", bins[8]));
    counts.extend([("black", black), ("white", white), ("gray", gray)]);
    counts.sort_by(|a, b| b.1.total_cmp(&a.1));
    let total: f32 = counts.iter().map(|(_, count)| *count).sum();
    if total <= 0.0 {
        return vec![ColorShare {
            color: "gray",
            ratio: 1.0,
        }];
    }
    let mut result: Vec<ColorShare> = counts
        .into_iter()
        .filter_map(|(color, count)| {
            let ratio = count / total;
            (ratio >= MIN_COLOR_RATIO).then_some(ColorShare { color, ratio })
        })
        .take(MAX_COLORS)
        .collect();
    if result.is_empty() {
        result.push(ColorShare {
            color: "gray",
            ratio: 1.0,
        });
    }
    result
}

#[cfg(test)]
#[path = "../../tests/unit/import_pixels.rs"]
mod tests;
