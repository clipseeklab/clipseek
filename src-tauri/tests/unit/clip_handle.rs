use super::*;

#[test]
fn cached_text_queries_do_not_initialize_models() {
    let handle = ClipHandle::default();
    handle
        .text_cache
        .lock()
        .unwrap()
        .insert("汽车".into(), vec![1.0, 0.0]);
    assert_eq!(handle.encode_text_cached(" 汽车 ").unwrap(), [1.0, 0.0]);
    assert!(handle.inner.inspect(|_| true).is_none());
    assert!(!handle.visual_ready());
}

#[test]
#[ignore = "requires local ONNX models and ORT; real unload/reload and inference"]
fn real_models_unload_and_reload_without_changing_embeddings() {
    fn rss_kib() -> String {
        std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &std::process::id().to_string()])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
            .unwrap_or_else(|| "unavailable".into())
    }

    let handle = ClipHandle::default();
    let image = image::DynamicImage::new_rgb8(224, 224);
    eprintln!("executed: before_load_rss_kib={}", rss_kib());
    let model = handle.get_or_init().unwrap();
    let text = handle.encode_text_cached("汽车").unwrap();
    let visual = model.encode_images(std::slice::from_ref(&image)).unwrap();
    assert!(handle.visual_ready());
    eprintln!("executed: loaded_rss_kib={}", rss_kib());
    // A live user of the encoder protects both sessions even when the timeout is zero.
    assert!(!handle.inner.release_if_idle(Instant::now(), Duration::ZERO));
    let old = Arc::downgrade(&model);
    drop(model);
    assert!(handle.inner.release_if_idle(Instant::now(), Duration::ZERO));
    assert!(
        old.upgrade().is_none(),
        "the actual encoder must have been dropped"
    );
    assert!(!handle.visual_ready());
    eprintln!("executed: unloaded_rss_kib={}", rss_kib());
    assert!(handle
        .inner
        .get_for_warmup(|| panic!("status polling must not reload models"))
        .unwrap()
        .is_none());
    assert_eq!(handle.encode_text_cached("汽车").unwrap(), text);
    assert!(handle.inner.inspect(|_| true).is_none());
    let reloaded = handle.get_or_init().unwrap();
    // Text-only reload does not eagerly create a visual session.
    assert!(!handle.visual_ready());
    let repeated_text = reloaded.encode_texts(&["汽车"]).unwrap().remove(0);
    let repeated_visual = reloaded.encode_images(&[image]).unwrap();
    assert!(handle.visual_ready());
    for (before, after) in text.iter().zip(repeated_text.iter()) {
        assert!((before - after).abs() < 1e-5);
    }
    assert_eq!(visual.len(), 1);
    assert_eq!(visual[0].len(), 768);
    for (before, after) in visual[0].iter().zip(repeated_visual[0].iter()) {
        assert!((before - after).abs() < 1e-4);
    }
    eprintln!("executed: reloaded_rss_kib={}", rss_kib());
}
