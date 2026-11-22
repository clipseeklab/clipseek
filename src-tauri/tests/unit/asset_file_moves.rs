use super::*;

#[test]
fn unwinding_moves_the_file_back_to_its_original_path() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    fs::write(&source, b"original").unwrap();
    let result = std::panic::catch_unwind(|| {
        let mut moves = FileMoves::default();
        moves.move_to(&source, &target).unwrap();
        panic!("database worker failed");
    });
    assert!(result.is_err());
    assert_eq!(fs::read(source).unwrap(), b"original");
    assert!(!target.exists());
}

#[test]
fn compensation_reports_conflicts_without_overwriting_either_file_and_continues() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("source");
    let target = dir.path().join("target");
    let other = dir.path().join("other");
    let other_target = dir.path().join("other-target");
    fs::write(&source, b"original").unwrap();
    fs::write(&other, b"other original").unwrap();
    let mut moves = FileMoves::default();
    moves.move_to(&other, &other_target).unwrap();
    moves.move_to(&source, &target).unwrap();
    fs::write(&source, b"external writer").unwrap();
    let error = moves.rollback().unwrap_err();
    assert_eq!(error.code(), "asset_rollback_failed");
    assert!(error.to_string().contains(target.to_str().unwrap()));
    assert_eq!(fs::read(source).unwrap(), b"external writer");
    assert_eq!(fs::read(target).unwrap(), b"original");
    assert_eq!(fs::read(other).unwrap(), b"other original");
    assert!(!other_target.exists());
}
