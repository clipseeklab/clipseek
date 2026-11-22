#![cfg(all(target_os = "macos", target_arch = "aarch64"))]
use clip_studio_lib::embedding::{
    domain::{Precision, Settings},
    engine::{initialize, Engine},
    execution::{Backend, BackendPreference, ExecutionOptions},
};
use std::time::Instant;

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), 768);
    assert_eq!(b.len(), 768);
    assert!(a.iter().chain(b).all(|v| v.is_finite()));
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

#[test]
#[ignore = "requires local Q4/Q8/FP32 models and a real Apple Silicon GPU; run explicitly"]
fn metal_executes_heavy_operators_for_every_precision_and_matches_cpu() {
    initialize(&clip_studio_lib::model_download::runtime()).unwrap();
    let root = std::env::var("CLIPSTUDIO_MODELS").expect("local model root");
    let image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        320,
        192,
        image::Rgb([255, 0, 0]),
    ));
    for precision in [Precision::Q8, Precision::Q4, Precision::Fp32] {
        let temp = tempfile::tempdir().unwrap();
        let settings = Settings {
            precision,
            model_directory: root.clone(),
        };
        let mut gpu = Engine::load_with_options(
            &settings,
            &ExecutionOptions {
                preference: BackendPreference::Metal,
                profile_directory: Some(temp.path().into()),
            },
        )
        .unwrap();
        assert_eq!(gpu.execution.backend, Backend::Metal);
        assert!(gpu.execution.fallback_reason.is_none());
        let mut cpu = Engine::load_with_options(
            &settings,
            &ExecutionOptions {
                preference: BackendPreference::Cpu,
                ..Default::default()
            },
        )
        .unwrap();
        let query = "a solid red image";
        let gpu_text = gpu.query(query).unwrap();
        let cpu_text = cpu.query(query).unwrap();
        assert!(
            cosine(&gpu_text, &cpu_text) > 0.9999,
            "{precision:?} text mismatch"
        );
        let start = Instant::now();
        let gpu_image = gpu.image(&image).unwrap();
        let gpu_time = start.elapsed();
        let start = Instant::now();
        let warm_image = gpu.image(&image).unwrap();
        let gpu_warm_time = start.elapsed();
        assert!(cosine(&gpu_image, &warm_image) > 0.9999);
        let start = Instant::now();
        let cpu_image = cpu.image(&image).unwrap();
        let cpu_time = start.elapsed();
        assert!(
            cosine(&gpu_image, &cpu_image) > 0.9999,
            "{precision:?} image mismatch"
        );
        assert!(gpu_text
            .iter()
            .zip(&gpu_image)
            .map(|(a, b)| a * b)
            .sum::<f32>()
            .is_finite());
        for profile in gpu.end_profiling().unwrap() {
            let events: Vec<serde_json::Value> =
                serde_json::from_slice(&std::fs::read(&profile).unwrap()).unwrap();
            let heavy = events
                .iter()
                .filter(|event| {
                    let args = &event["args"];
                    args["provider"] == "WebGpuExecutionProvider"
                        && matches!(
                            args["op_name"].as_str(),
                            Some("MatMulNBits" | "MatMul" | "Gemm")
                        )
                })
                .count();
            assert!(heavy > 0, "No heavy GPU operators in {}", profile.display());
            println!(
                "{precision:?}: {} heavy GPU events in {}",
                heavy,
                profile.file_name().unwrap().to_string_lossy()
            );
        }
        println!(
            "{precision:?}: image Metal cold={gpu_time:?} warm={gpu_warm_time:?} CPU={cpu_time:?}"
        );
    }
}
