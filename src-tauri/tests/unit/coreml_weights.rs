use super::*;
fn span(root: &Path, name: &str, bytes: &[u8], shape: Vec<usize>, dtype: &str) -> Span {
    fs::create_dir_all(root.join("onnx")).unwrap();
    fs::write(root.join("onnx").join(name), bytes).unwrap();
    Span {
        file: name.into(),
        offset: 0,
        bytes: bytes.len(),
        dtype: dtype.into(),
        shape,
    }
}
#[test]
fn q4_odd_zero_point_groups_preserve_orientation_and_original_bytes() {
    let root = tempfile::tempdir().unwrap();
    let data = span(
        root.path(),
        "q.bin",
        &[0x21, 0x43, 0x65, 0x87, 0xa9, 0xcb],
        vec![2, 3, 1],
        "uint8",
    );
    let scales = span(
        root.path(),
        "s.bin",
        &(1..=6)
            .flat_map(|x| (x as f32).to_le_bytes())
            .collect::<Vec<_>>(),
        vec![2, 3],
        "float32",
    );
    let zeros = span(
        root.path(),
        "z.bin",
        &[0x21, 0xf3, 0x54, 0xf6],
        vec![2, 2],
        "uint8",
    );
    let mut recipe = Recipe {
        kind: "matmul".into(),
        data,
        scales: Some(scales),
        zeros: Some(zeros),
        bits: Some(4),
        k: Some(5),
        n: Some(2),
        block: Some(2),
    };
    assert_eq!(
        values(root.path(), &recipe).unwrap(),
        vec![0., 12., 1., 16., 2., 20., 4., 25., 6., 30.]
    );
    recipe.kind = "gather".into();
    recipe.data.shape = vec![2, 3];
    assert_eq!(
        values(root.path(), &recipe).unwrap(),
        vec![0., 1., 2., 4., 6., 9., 12., 16., 20., 25., 30., 36.]
    );
    assert_eq!(
        fs::read(root.path().join("onnx/q.bin")).unwrap(),
        [0x21, 0x43, 0x65, 0x87, 0xa9, 0xcb]
    );
}
#[test]
fn source_ranges_and_paths_are_bounded() {
    let root = tempfile::tempdir().unwrap();
    let mut s = span(root.path(), "source.bin", &[1, 2, 3, 4], vec![4], "uint8");
    s.offset = 1;
    assert!(read(root.path(), &s).is_err());
    for name in [".", "..", "../source.bin", "/source.bin"] {
        assert!(source_path(root.path(), name).is_err());
    }
    assert!(plain(&[1, 2, 3], "float32").is_err());
}
