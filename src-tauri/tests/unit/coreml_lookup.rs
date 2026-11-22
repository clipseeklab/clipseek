use super::*;
fn span(root: &Path, name: &str, bytes: &[u8], shape: Vec<usize>, dtype: &str) -> Span {
    std::fs::create_dir_all(root.join("onnx")).unwrap();
    std::fs::write(root.join("onnx").join(name), bytes).unwrap();
    Span {
        file: name.into(),
        offset: 0,
        bytes: bytes.len(),
        dtype: dtype.into(),
        shape,
    }
}
#[test]
fn sparse_q4_rows_use_each_original_scale_and_nibble() {
    let root = tempfile::tempdir().unwrap();
    let data = span(root.path(), "q", &vec![0x21; 512], vec![2, 256], "uint8");
    let scales = span(
        root.path(),
        "s",
        &(0..32)
            .flat_map(|i| (if i < 16 { 2f32 } else { 3f32 }).to_le_bytes())
            .collect::<Vec<_>>(),
        vec![2, 16],
        "float32",
    );
    let zeros = span(root.path(), "z", &[0x11; 16], vec![2, 8], "uint8");
    let recipe = Recipe {
        kind: "gather".into(),
        data,
        scales: Some(scales),
        zeros: Some(zeros),
        bits: Some(4),
        block: Some(32),
        k: None,
        n: None,
    };
    let table = Table::open(root.path(), &recipe, 512).unwrap();
    assert_eq!(&table.row(0).unwrap()[..4], &[0., 2., 0., 2.]);
    assert_eq!(&table.row(1).unwrap()[..4], &[0., 3., 0., 3.]);
    assert!(table.row(-1).is_err());
    assert!(table.row(2).is_err());
    assert_eq!(
        std::fs::read(root.path().join("onnx/q")).unwrap(),
        vec![0x21; 512]
    );
}
#[test]
fn plain_rows_are_not_normalized_or_rounded() {
    let root = tempfile::tempdir().unwrap();
    let data = span(
        root.path(),
        "f",
        &(0..1024)
            .flat_map(|i| ((i as f32) * 0.00012345).to_le_bytes())
            .collect::<Vec<_>>(),
        vec![2, 512],
        "float32",
    );
    let recipe = Recipe {
        kind: "plain".into(),
        data,
        scales: None,
        zeros: None,
        bits: None,
        block: None,
        k: None,
        n: None,
    };
    let table = Table::open(root.path(), &recipe, 512).unwrap();
    assert_eq!(table.row(1).unwrap()[3], 515f32 * 0.00012345);
}
