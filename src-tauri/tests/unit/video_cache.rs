use super::FrameCache;
use image::{Rgb, RgbImage};

#[test]
fn repeated_inputs_preserve_every_token_and_feature_without_repeating_vision() {
    let mut cache = FrameCache::default();
    let pixels = RgbImage::from_pixel(48, 48, Rgb([0, 0, 255]));
    let expected = vec![0.25; 512];
    let mut features = Vec::new();
    let mut counts = Vec::new();
    for frame in 0..32 {
        let (values, count) = cache
            .encode(pixels.clone(), |_| {
                assert_eq!(
                    frame, 0,
                    "Identical adjacent pixels must reuse vision features"
                );
                Ok(expected.clone())
            })
            .unwrap();
        counts.push(count);
        features.extend_from_slice(values);
    }
    assert_eq!(counts, vec![1; 32]);
    assert_eq!(features, expected.repeat(32));
}

#[test]
fn changed_pixels_or_dimensions_are_encoded_again_and_only_one_frame_is_cached() {
    let blue = RgbImage::from_pixel(48, 48, Rgb([0, 0, 255]));
    let mut changed = blue.clone();
    changed.put_pixel(0, 0, Rgb([0, 0, 254]));
    let mut cache = FrameCache::default();
    let mut calls = 0;
    for pixels in [blue.clone(), changed.clone(), changed, blue] {
        cache
            .encode(pixels, |_| {
                calls += 1;
                Ok(vec![calls as f32; 512])
            })
            .unwrap();
    }
    assert_eq!(calls, 3);
    for (width, height) in [(48, 96), (96, 48)] {
        let (_, count) = cache
            .encode(
                RgbImage::from_pixel(width, height, Rgb([0, 0, 255])),
                |_| {
                    calls += 1;
                    Ok(vec![0.; 1024])
                },
            )
            .unwrap();
        assert_eq!(count, 2);
    }
    assert_eq!(calls, 5);
}

#[test]
fn a_failed_vision_call_is_never_cached() {
    let pixels = RgbImage::from_pixel(48, 48, Rgb([255, 0, 0]));
    let mut cache = FrameCache::default();
    assert!(cache
        .encode(pixels.clone(), |_| anyhow::bail!(
            "synthetic inference failure"
        ))
        .is_err());
    let (features, count) = cache.encode(pixels, |_| Ok(vec![0.5; 512])).unwrap();
    assert_eq!(features, vec![0.5; 512]);
    assert_eq!(count, 1);
}
