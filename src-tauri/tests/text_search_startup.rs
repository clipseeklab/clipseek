//! Run explicitly with --ignored --nocapture to measure real model cold-start cost.
use clip_studio_lib::clip::{resolve_models_dir, ClipHandle};
use std::time::Instant;

#[test]
#[ignore = "requires local ONNX models; measures real cold-start latency"]
fn text_search_cold_start() {
    assert!(resolve_models_dir().is_some(), "local models required");
    let handle = ClipHandle::default();
    let start = Instant::now();
    let clip = handle.get_or_init().unwrap();
    let loaded = start.elapsed();
    let vector = clip.encode_texts(&["汽车"]).unwrap().remove(0);
    let first = start.elapsed();
    let warm = Instant::now();
    let repeated = clip.encode_texts(&["汽车"]).unwrap().remove(0);
    eprintln!(
        "text search: init={loaded:?}, first={first:?}, warm={:?}",
        warm.elapsed()
    );
    assert_eq!(vector.len(), 768);
    assert!((vector.iter().map(|x| x * x).sum::<f32>() - 1.0).abs() < 1e-3);
    assert!(vector
        .iter()
        .zip(repeated)
        .all(|(a, b)| (a - b).abs() < 1e-5));
}

#[test]
#[ignore = "requires local ONNX models"]
fn concurrent_initialization_shares_one_encoder() {
    use std::sync::{Arc, Barrier};
    let handle = Arc::new(ClipHandle::default());
    let barrier = Arc::new(Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let handle = handle.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                handle.get_or_init().unwrap()
            })
        })
        .collect();
    let encoders: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert!(encoders
        .iter()
        .all(|encoder| Arc::ptr_eq(encoder, &encoders[0])));
}
