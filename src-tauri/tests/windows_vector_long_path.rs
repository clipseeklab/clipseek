#![cfg(target_os = "windows")]

use std::{collections::HashMap, fs, path::PathBuf};

use clip_studio_lib::vector::{open_index, VECTOR_NAME};
use qdrant_edge::{
    EdgeShard, NamedQuery, PointId, PointInsertOperations, PointOperations, PointStruct, QueryEnum,
    QueryRequest, ScoringQuery, UpdateOperation, Vector,
};
use serde_json::json;

fn write_and_search(shard: &EdgeShard, id: u64, expected: usize) {
    let vectors: qdrant_edge::Vectors =
        HashMap::from([(VECTOR_NAME.to_owned(), Vector::new_dense(vec![1.0; 768]))]).into();
    let point = PointStruct::new(PointId::NumId(id), vectors, json!({"asset_id": id}));
    shard
        .update(UpdateOperation::PointOperation(
            PointOperations::UpsertPoints(PointInsertOperations::PointsList(vec![point.into()])),
        ))
        .expect("write vectors at a deep Windows path");
    shard.flush().unwrap();
    shard.optimize().expect("optimize at a deep Windows path");
    assert_search(shard, expected);
}

fn assert_search(shard: &EdgeShard, expected: usize) {
    let mut request = QueryRequest::new(10);
    request.query = Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery::new(
        Vector::new_dense(vec![1.0; 768]).into(),
        VECTOR_NAME,
    ))));
    assert_eq!(shard.query(request).unwrap().len(), expected);
}

fn exercise_deep_index(min_root_length: usize) {
    let temp = tempfile::tempdir().unwrap();
    // Always pass the ordinary path to the application, even when TEMP itself
    // uses extended-length syntax. Each component stays below NTFS's limit.
    let canonical = fs::canonicalize(temp.path()).unwrap();
    let canonical = canonical.to_string_lossy();
    let ordinary = if let Some(unc) = canonical.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        canonical.trim_start_matches(r"\\?\").to_owned()
    };
    let mut root = PathBuf::from(ordinary);
    while root.as_os_str().encode_wide().count() < min_root_length {
        root = root.join("深层素材库 with spaces");
    }
    let dir = root.join(".clipstudio-open").join("vector");
    assert!(!dir.to_string_lossy().starts_with(r"\\?\"));
    let shard = open_index(&dir).expect("initialize an index below a deep library root");
    write_and_search(&shard, 1, 1);
    drop(shard);

    let shard = open_index(&dir).expect("reopen the persisted index using the ordinary path");
    assert_search(&shard, 1);
    write_and_search(&shard, 2, 2);
    drop(shard);
    // Explicit extended path also makes cleanup independent of MAX_PATH.
    fs::remove_dir_all(fs::canonicalize(temp.path()).unwrap()).unwrap();
}

use std::os::windows::ffi::OsStrExt;

#[test]
fn deep_root_with_long_internal_temporary_paths() {
    exercise_deep_index(160);
}

#[test]
fn deeper_root_preserves_vectors_after_reopen() {
    exercise_deep_index(200);
}

#[test]
fn index_path_itself_exceeds_max_path() {
    exercise_deep_index(280);
}
