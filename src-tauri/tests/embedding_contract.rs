use clip_studio_lib::{
    config::Settings,
    embedding::domain::{normalize, validate_duration, Precision, DIM},
    import::{self, ImportMode},
    library,
};
use std::sync::{atomic::AtomicBool, Arc};

#[test]
fn defaults_are_q8_reference_and_visual_only() {
    let s = Settings::default();
    assert_eq!(s.language, "en");
    assert_eq!(s.embedding_precision, Precision::Q8);
    assert_eq!(s.import_mode, "reference");
    assert!(!s.auto_subtitle_index);
    for precision in [Precision::Q4, Precision::Q8, Precision::Fp32] {
        assert_eq!(
            serde_json::from_str::<Precision>(&format!("\"{}\"", precision.key())).unwrap(),
            precision
        );
    }
}
#[test]
fn video_admission_accepts_32_seconds_and_rejects_invalid_or_longer_inputs() {
    for value in [0.01, 1., 31.999, 32.] {
        validate_duration(value).unwrap();
    }
    for value in [0., -1., 32.000001, 33., f64::NAN, f64::INFINITY] {
        assert!(validate_duration(value).is_err(), "{value}");
    }
}
#[test]
fn vectors_must_have_the_shared_dimension_and_finite_unit_norm() {
    assert!(normalize(vec![0.; DIM]).is_err());
    assert!(normalize(vec![1.; 512]).is_err());
    assert!(normalize(vec![f32::NAN; DIM]).is_err());
    let v = normalize(vec![2.; DIM]).unwrap();
    assert!((v.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 1e-5);
}
#[test]
fn copy_import_cannot_create_a_row_or_touch_the_original() {
    let temp = tempfile::tempdir().unwrap();
    let input = temp.path().join("photo.png");
    image::RgbImage::new(4, 4).save(&input).unwrap();
    let before = std::fs::read(&input).unwrap();
    let meta = library::init_library(&temp.path().join("library"), "Test").unwrap();
    let s = Arc::new(library::open_session(meta).unwrap());
    let result = import::register_files(
        &s,
        std::slice::from_ref(&input),
        None,
        &AtomicBool::new(false),
        ImportMode::Copy,
        &|_, _, _| {},
        &|_| {},
    );
    assert!(result.pending.is_empty());
    assert_eq!(
        s.read_db()
            .query_row("SELECT COUNT(*) FROM assets", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    assert_eq!(std::fs::read(input).unwrap(), before);
}

use clip_studio_lib::embedding::preprocess;

#[test]
fn video_tokens_are_one_joint_sequence() {
    let ids = preprocess::media_tokens(&[126, 126, 126], true);
    assert_eq!(ids.iter().filter(|&&id| id == 258884).count(), 378);
    assert_eq!(ids.iter().filter(|&&id| id == 2).count(), 1);
    assert_eq!(ids.iter().filter(|&&id| id == 1).count(), 1);
    assert!(!ids.contains(&258880));
}

#[test]
fn patches_preserve_coordinates_and_exclude_padding() {
    let image = image::RgbImage::from_pixel(48, 48, image::Rgb([255, 0, 128]));
    let patches = preprocess::patchify_resized(&image, 140).unwrap();
    assert_eq!(patches.soft_tokens, 1);
    assert_eq!(&patches.positions[..8], &[0, 0, 1, 0, 2, 0, 0, 1]);
    assert_eq!(&patches.positions[18..22], &[-1, -1, -1, -1]);
    assert_eq!(&patches.pixels[..3], &[1., 0., 128. / 255.]);
    assert!(patches.pixels[9 * 768..].iter().all(|v| *v == 0.));
}

#[test]
fn aspect_ratio_extremes_stay_within_token_budget() {
    for (w, h) in [(1, 100000), (100000, 1), (1920, 1080), (1080, 1920), (1, 1)] {
        let (tw, th) = preprocess::target_size(w, h, 140).unwrap();
        assert!(tw > 0 && th > 0 && tw % 48 == 0 && th % 48 == 0);
        assert!(tw as usize * th as usize / 256 <= 140 * 9);
    }
}

#[test]
fn gif_reference_import_uses_one_still_index_unit_without_ffmpeg_extraction() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("image.gif");
    image::RgbaImage::from_pixel(12, 12, image::Rgba([220, 30, 40, 255]))
        .save(&source)
        .unwrap();
    let meta = library::init_library(&temp.path().join("library"), "GIF").unwrap();
    let s = Arc::new(library::open_session(meta).unwrap());
    let result = import::run_import(
        &s,
        std::slice::from_ref(&source),
        None,
        &AtomicBool::new(false),
        ImportMode::Reference,
        &|_, _, _| {},
        &|_| {},
    );
    assert_eq!(result.done, 1, "{result:?}");
    assert_eq!(
        s.read_db()
            .query_row("SELECT COUNT(*) FROM frames", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(source.exists());
}

#[test]
fn automatic_placement_prefers_native_coreml_only_on_apple_silicon_and_cpu_remains_explicit() {
    use clip_studio_lib::embedding::execution::{
        preferred_backend, Backend, BackendPreference, ExecutionOptions,
    };
    let apple_silicon = cfg!(all(target_os = "macos", target_arch = "aarch64"));
    assert_eq!(
        preferred_backend(BackendPreference::Auto).unwrap(),
        if apple_silicon {
            Backend::Coreml
        } else {
            Backend::Cpu
        }
    );
    assert_eq!(
        preferred_backend(BackendPreference::Cpu).unwrap(),
        Backend::Cpu
    );
    assert_eq!(
        preferred_backend(BackendPreference::Metal).is_ok(),
        apple_silicon
    );
    assert_eq!(
        preferred_backend(BackendPreference::Coreml).is_ok(),
        apple_silicon
    );
    let options = ExecutionOptions::default();
    assert_eq!(options.preference, BackendPreference::Auto);
    assert!(options.profile_directory.is_none());
}
