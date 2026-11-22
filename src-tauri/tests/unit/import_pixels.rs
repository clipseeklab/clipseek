use super::*;

#[test]
fn filters_solid_and_sparse_noise() {
    let mut solid = vec![128u8; 96 * 96 * 3];
    assert!(is_low_information(96, 96, &solid));
    for i in 0..30 {
        solid[i * 97] = 255;
    }
    assert!(is_low_information(96, 96, &solid));
}

#[test]
fn keeps_structured_frame() {
    let mut image = vec![0u8; 96 * 96 * 3];
    for y in 0..96usize {
        for x in 0..96usize {
            let value = if (x / 8 + y / 8) % 2 == 0 { 25 } else { 225 };
            let offset = (y * 96 + x) * 3;
            image[offset..offset + 3].fill(value);
        }
    }
    assert!(!is_low_information(96, 96, &image));
}

#[test]
fn landscape_dhash_is_not_always_zero() {
    let mut image = vec![0u8; 160 * 90 * 3];
    for y in 0..90usize {
        for x in 0..160usize {
            let offset = (y * 160 + x) * 3;
            image[offset..offset + 3].fill((x * 255 / 159) as u8);
        }
    }
    assert_ne!(dhash(160, 90, &image), 0);
}

#[test]
fn keeps_only_top_three_colors_above_threshold() {
    let mut data = Vec::new();
    for (rgb, count) in [
        ([255, 0, 0], 40),
        ([0, 0, 255], 30),
        ([0, 255, 0], 20),
        ([255, 255, 0], 10),
    ] {
        for _ in 0..count {
            data.extend_from_slice(&rgb);
        }
    }
    let colors = dominant_colors(100, 1, &data);
    assert_eq!(
        colors.iter().map(|c| c.color).collect::<Vec<_>>(),
        vec!["red", "blue", "green"]
    );
    assert!((colors[0].ratio - 0.4).abs() < 0.001);
}

#[test]
fn ignores_color_below_ten_percent() {
    let mut data = Vec::new();
    for _ in 0..91 {
        data.extend_from_slice(&[255, 0, 0]);
    }
    for _ in 0..9 {
        data.extend_from_slice(&[0, 0, 255]);
    }
    let colors = dominant_colors(100, 1, &data);
    assert_eq!(colors.len(), 1);
    assert_eq!(colors[0].color, "red");
}
