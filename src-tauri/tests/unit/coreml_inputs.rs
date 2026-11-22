use super::*;
fn fixture() -> (tempfile::TempDir, Metadata, lookup::Table) {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("onnx")).unwrap();
    let bytes: Vec<_> = (0..5)
        .flat_map(|row| std::iter::repeat_n(row as f32, 512).flat_map(f32::to_le_bytes))
        .collect();
    std::fs::write(root.path().join("onnx/rows"), &bytes).unwrap();
    let recipe = weights::Recipe {
        kind: "plain".into(),
        data: weights::Span {
            file: "rows".into(),
            offset: 0,
            bytes: bytes.len(),
            dtype: "float32".into(),
            shape: vec![5, 512],
        },
        scales: None,
        zeros: None,
        bits: None,
        k: None,
        n: None,
        block: None,
    };
    let table = lookup::Table::open(root.path(), &recipe, 512).unwrap();
    let metadata = Metadata {
        revision: MODEL_REVISION.into(),
        buckets: cache::BUCKETS.to_vec(),
        width: 512,
        dimension: DIM,
        image_token: 3,
        video_token: 4,
        audio_token: Some(5),
        recipes: HashMap::new(),
    };
    (root, metadata, table)
}
#[test]
fn padding_preserves_tokens_visual_order_and_excludes_unused_rows() {
    let (_root, m, t) = fixture();
    let features: Vec<_> = std::iter::repeat_n(7f32, 512)
        .chain(std::iter::repeat_n(9f32, 512))
        .collect();
    let p = prepare(&m, &t, &[0, 3, 3, 1], &features, false).unwrap();
    assert_eq!(p.bucket, 32);
    assert_eq!(&p.ids[..4], &[0, 3, 3, 1]);
    assert_eq!(&p.mask[..5], &[1, 1, 1, 1, 0]);
    assert_eq!(&p.positions[..4], &[0., 1., 2., 3.]);
    assert!(p.embedded[512..1024].iter().all(|x| *x == 7.));
    assert!(p.embedded[1024..1536].iter().all(|x| *x == 9.));
    assert!(p.embedded[2048..].iter().all(|x| *x == 0.));
}
#[test]
fn context_limit_and_placeholder_validation_do_not_trigger_provider_fallback() {
    let (_root, m, t) = fixture();
    assert!(prepare(&m, &t, &[], &[], false).is_err());
    assert!(prepare(&m, &t, &vec![0; 8193], &[], false).is_err());
    assert_eq!(
        prepare(&m, &t, &vec![0; 8192], &[], false).unwrap().bucket,
        8192
    );
    for id in [3, 4, 5] {
        let error = prepare(&m, &t, &[id], &[], false).err().unwrap();
        assert!(!is_inference_failure(&error));
    }
    assert!(prepare(&m, &t, &[0], &vec![1.; 512], false).is_err());
}
#[test]
fn only_native_execution_errors_qualify_for_automatic_retry() {
    assert!(!is_inference_failure(&anyhow::anyhow!(
        "Indexing cancelled"
    )));
    assert!(is_inference_failure(
        &anyhow::anyhow!("GPU command failed").context(ExecutionFailure)
    ));
}
