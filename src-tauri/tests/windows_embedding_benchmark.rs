#![cfg(target_os = "windows")]

use clip_studio_lib::{
    clip::ClipEncoder,
    embedding::{
        domain::{Precision, Settings},
        execution::Backend,
    },
};
use std::{path::PathBuf, time::Instant};

#[test]
#[ignore = "requires Q8 models, synthetic video fixtures and an explicit benchmark output path"]
fn q8_video_benchmark_preserves_recorded_vectors() {
    let output = std::env::var("CLIPSTUDIO_BENCHMARK_OUTPUT").expect("benchmark output");
    let fixtures = PathBuf::from(std::env::var("CLIPSTUDIO_EMBEDDING_FIXTURES").expect("fixtures"));
    let encoder = ClipEncoder::load_settings(&Settings {
        precision: Precision::Q8,
        model_directory: std::env::var("CLIPSTUDIO_MODELS").expect("models"),
    })
    .unwrap();
    assert_eq!(encoder.execution_status().backend, Backend::Cpu);
    let reference: Option<serde_json::Value> = std::env::var("CLIPSTUDIO_BENCHMARK_REFERENCE")
        .ok()
        .map(|path| serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap());
    let mut results = serde_json::Map::new();
    for name in ["photo.png", "short.mp4", "limit.mp4"] {
        let start = Instant::now();
        let vector = encoder
            .encode_asset(&fixtures.join(name), name.ends_with(".mp4"), &|| false)
            .unwrap();
        let elapsed_ms = start.elapsed().as_millis();
        assert_eq!(vector.len(), 768);
        assert!(vector.iter().all(|x| x.is_finite()));
        let norm = vector.iter().map(|x| x * x).sum::<f32>();
        assert!((norm - 1.).abs() < 1e-4);
        if let Some(reference) = &reference {
            let expected = reference[name]["vector"].as_array().unwrap();
            assert_eq!(expected.len(), 768);
            let cosine = vector
                .iter()
                .zip(expected)
                .map(|(a, b)| f64::from(*a) * b.as_f64().unwrap())
                .sum::<f64>();
            assert!(cosine > 0.9999, "{name}: cosine={cosine}");
            eprintln!("{name}: {elapsed_ms}ms, cosine={cosine}");
        } else {
            eprintln!("{name}: {elapsed_ms}ms, recorded reference");
        }
        results.insert(
            name.into(),
            serde_json::json!({"elapsedMs": elapsed_ms, "vector": vector}),
        );
        std::fs::write(&output, serde_json::to_vec_pretty(&results).unwrap()).unwrap();
    }
}
