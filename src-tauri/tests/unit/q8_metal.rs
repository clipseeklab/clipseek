use super::*;
#[test]
fn protobuf_rejects_truncation_and_preserves_unknown_fields() {
    for data in [&[0x80][..], &[0x0a, 5, 1][..], &[0x0f][..]] {
        assert!(parse(data).is_err());
    }
    let original = encode(&[
        integer(901, 500),
        bytes(902, b"unknown metadata"),
        Field {
            tag: 903,
            wire: 5,
            data: vec![1, 2, 3, 4],
        },
    ]);
    assert_eq!(encode(&parse(&original).unwrap()), original);
}
#[test]
fn external_weights_reject_parent_paths_before_opening() {
    let fields = vec![
        bytes(
            13,
            encode(&[bytes(1, "location"), bytes(2, "../outside.bin")]),
        ),
        bytes(13, encode(&[bytes(1, "offset"), bytes(2, "0")])),
        bytes(13, encode(&[bytes(1, "length"), bytes(2, "4")])),
    ];
    let temp = tempfile::tempdir().unwrap();
    assert!(raw(&fields, temp.path())
        .unwrap_err()
        .to_string()
        .contains("Unsafe"));
}
