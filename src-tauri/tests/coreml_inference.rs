#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use clip_studio_lib::embedding::{
    domain::{Precision, Settings},
    engine::{initialize, Engine},
    execution::{Backend, BackendPreference, ExecutionOptions},
    media::Media,
};
use std::{path::PathBuf, time::Instant};
fn cosine(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), 768);
    assert_eq!(b.len(), 768);
    assert!(a.iter().chain(b).all(|v| v.is_finite()));
    let norm = |v: &[f32]| v.iter().map(|x| (*x as f64).powi(2)).sum::<f64>().sqrt();
    assert!((norm(a) - 1.).abs() < 1e-5);
    assert!((norm(b) - 1.).abs() < 1e-5);
    a.iter()
        .zip(b)
        .map(|(x, y)| *x as f64 * *y as f64)
        .sum::<f64>()
        / norm(a)
        / norm(b)
}
fn load(settings: &Settings, preference: BackendPreference) -> Engine {
    Engine::load_with_options(
        settings,
        &ExecutionOptions {
            preference,
            ..Default::default()
        },
    )
    .unwrap()
}
#[test]
#[ignore = "requires all precision models, generated video fixtures, and a physical Apple Silicon GPU"]
fn native_coreml_preserves_all_precisions_and_complete_media_paths() {
    let runtime = clip_studio_lib::model_download::runtime();
    initialize(&runtime).unwrap();
    let root = std::env::var("CLIPSTUDIO_MODELS").expect("local model root");
    let fixtures =
        PathBuf::from(std::env::var("CLIPSTUDIO_EMBEDDING_FIXTURES").expect("generated fixtures"));
    let motion = PathBuf::from(
        std::env::var("CLIPSTUDIO_MOTION_FIXTURE")
            .expect("generated changing-frame 32-second fixture"),
    );
    let media = Media {
        ffmpeg: runtime.join("ffmpeg"),
        ffprobe: runtime.join("ffprobe"),
    };
    for precision in [Precision::Q8, Precision::Q4, Precision::Fp32] {
        let settings = Settings {
            precision,
            model_directory: root.clone(),
        };
        let start = Instant::now();
        let mut native = load(&settings, BackendPreference::Coreml);
        println!("COREML {precision:?} load={:?}", start.elapsed());
        assert_eq!(native.execution.backend, Backend::Coreml);
        assert!(native.execution.fallback_reason.is_none());
        let mut cpu = load(&settings, BackendPreference::Cpu);
        for query in [
            "a solid red image".into(),
            "蓝色天空和绿色的森林".into(),
            "red ".repeat(2300),
            "red ".repeat(8000),
        ] {
            let start = Instant::now();
            let a = native.query(&query).unwrap();
            let elapsed = start.elapsed();
            let b = cpu.query(&query).unwrap();
            let c = cosine(&a, &b);
            println!(
                "COREML {precision:?} query_chars={} cosine={c:.12} time={elapsed:?}",
                query.len()
            );
            assert!(c > 0.9999, "{precision:?} text mismatch: {c}");
        }
        for (width, height, color) in [
            (320, 192, [255, 0, 0]),
            (192, 320, [0, 0, 255]),
            (256, 256, [12, 192, 47]),
            (1, 10000, [255, 127, 0]),
        ] {
            let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                width,
                height,
                image::Rgb(color),
            ));
            let a = native.image(&image).unwrap();
            let b = cpu.image(&image).unwrap();
            let c = cosine(&a, &b);
            println!("COREML {precision:?} image={width}x{height} cosine={c:.12}");
            assert!(c > 0.9999, "{precision:?} image mismatch: {c}");
        }
        let mut seed = 0x12345678u32;
        let noise = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(641, 359, |_, _| {
            let mut pixel = [0; 3];
            for channel in &mut pixel {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                *channel = (seed >> 24) as u8;
            }
            image::Rgb(pixel)
        }));
        let a = native.image(&noise).unwrap();
        let b = cpu.image(&noise).unwrap();
        assert!(cosine(&a, &b) > 0.9999, "{precision:?} noise mismatch");
        for path in [
            fixtures.join("short.mp4"),
            fixtures.join("limit.mp4"),
            motion.clone(),
        ] {
            let before = std::fs::read(&path).unwrap();
            let start = Instant::now();
            let a = native.asset(&path, true, &media, &|| true).unwrap();
            let elapsed = start.elapsed();
            let b = cpu.asset(&path, true, &media, &|| true).unwrap();
            let c = cosine(&a, &b);
            println!(
                "COREML {precision:?} video={} cosine={c:.12} time={elapsed:?}",
                path.file_name().unwrap().to_string_lossy()
            );
            assert!(c > 0.9999, "{precision:?} video mismatch: {c}");
            assert_eq!(before, std::fs::read(&path).unwrap());
        }
        assert!(native
            .asset(&fixtures.join("too-long.mp4"), true, &media, &|| true)
            .is_err());
        assert!(native
            .asset(&fixtures.join("limit.mp4"), true, &media, &|| false)
            .is_err());
        assert_eq!(native.execution.backend, Backend::Coreml);
        assert!(native.execution.fallback_reason.is_none());
        assert!(native.query(&"red ".repeat(9000)).is_err());
        assert!(native.query(" ").is_err());
        assert_eq!(native.execution.backend, Backend::Coreml);
        drop(cpu);
        drop(native);
        let start = Instant::now();
        let mut cached = load(&settings, BackendPreference::Coreml);
        let v = cached.query("a red image").unwrap();
        assert_eq!(v.len(), 768);
        println!(
            "COREML {precision:?} verified_cached_load={:?}",
            start.elapsed()
        );
    }
}

#[test]
#[ignore = "requires Q8 models and a physical Apple Silicon GPU; mutates only a generated cache"]
fn automatic_native_failure_retries_metal_and_updates_the_shared_encoder_status() {
    use clip_studio_lib::clip::ClipEncoder;
    let settings = Settings {
        precision: Precision::Q8,
        model_directory: std::env::var("CLIPSTUDIO_MODELS").expect("local model root"),
    };
    let encoder = ClipEncoder::load_settings(&settings).unwrap();
    assert_eq!(encoder.execution_status().backend, Backend::Coreml);
    assert!(encoder.encode_texts(&[" "]).is_err());
    assert_eq!(encoder.execution_status().backend, Backend::Coreml);
    let root = PathBuf::from(&settings.model_directory).join("q8");
    let cache = std::fs::read_dir(root)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("coreml-gpu-v2-q8-")
        })
        .unwrap();
    let original = cache.join("body-8192.mlmodelc");
    let hidden = cache.join("body-8192.mlmodelc-test-hidden");
    struct Restore(PathBuf, PathBuf);
    impl Drop for Restore {
        fn drop(&mut self) {
            std::fs::rename(&self.1, &self.0).expect("restore generated cache");
        }
    }
    std::fs::rename(&original, &hidden).unwrap();
    let restore = Restore(original, hidden);
    let result = encoder.encode_texts(&[&"red ".repeat(5000)]).unwrap();
    drop(restore);
    assert_eq!(result[0].len(), 768);
    assert!(result[0].iter().all(|v| v.is_finite()));
    let status = encoder.execution_status();
    assert_eq!(status.backend, Backend::Metal);
    assert!(status
        .fallback_reason
        .unwrap()
        .contains("Native CoreML inference failed"));
    assert_eq!(encoder.visual_provider(), "Metal GPU (WebGPU)");
}

#[test]
#[ignore = "requires all precision models and a physical Apple Silicon GPU"]
fn native_long_context_preserves_varied_multilingual_content() {
    initialize(&clip_studio_lib::model_download::runtime()).unwrap();
    let root = std::env::var("CLIPSTUDIO_MODELS").expect("local model root");
    let passages = [
        "At sunrise a cyclist crosses a wooden bridge beside a quiet river. The camera turns toward distant mountains, then follows a dog running through a green field. Indoor scenes show books, tools, food, and people discussing a map. ",
        "清晨的街道上，一位骑车的人经过木桥和河流。镜头转向远处的山峰，随后出现树林、海边、书架与厨房。一只小狗穿过草地，人们在地图前讨论旅行路线。",
    ];
    for precision in [Precision::Q8, Precision::Q4, Precision::Fp32] {
        let settings = Settings {
            precision,
            model_directory: root.clone(),
        };
        let mut native = load(&settings, BackendPreference::Coreml);
        let mut cpu = load(&settings, BackendPreference::Cpu);
        for (language, passage) in passages.iter().enumerate() {
            for repetitions in [35, 75] {
                let query = passage.repeat(repetitions);
                let a = native.query(&query).unwrap();
                let b = cpu.query(&query).unwrap();
                let c = cosine(&a, &b);
                println!("COREML varied {precision:?} language={language} repetitions={repetitions} cosine={c:.12}");
                assert!(
                    c > 0.9999,
                    "{precision:?} varied long context mismatch: {c}"
                );
            }
        }
        assert_eq!(native.execution.backend, Backend::Coreml);
    }
}
