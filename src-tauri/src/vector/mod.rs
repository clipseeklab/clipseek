//! Embedded Qdrant EdgeShard per library under .clipstudio-open/vector. The named vector clip
//! contains normalized 768-dimensional embeddings. Point IDs match frames.id and payloads
//! carry structured filters. Explicit optimization runs after batches because EdgeShard has
//! no background optimizer.

mod embedding;
pub use embedding::{embed_pending, embed_pending_while};

use std::path::Path;

use ordered_float::OrderedFloat;
use qdrant_edge::{
    Condition, Distance, EdgeConfigBuilder, EdgeShard, EdgeVectorParamsBuilder, FieldCondition,
    Filter, HnswIndexConfig, IsNullCondition, JsonPath, Match, NamedQuery, Payload, PayloadOps,
    PointId, PointInsertOperations, PointOperations, PointStruct, QueryEnum, QueryRequest, Range,
    RangeInterface, RetrieveRequest, ScalarQuantizationConfig, ScoringQuery, SetPayloadOp,
    UpdateOperation, ValueVariants, Vector, WithPayloadInterface,
};
use serde_json::{json, Map, Value as JsonValue};

use crate::clip::EMBED_DIM;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;

pub mod recovery;

/// Named vector identifier shared with index creation.
pub const VECTOR_NAME: &str = "clip";

/// Collect directory structure and sizes without logging filenames or library paths,
/// supporting WAL, segment, permissions, and free-space diagnostics.
pub fn index_storage_summary(dir: &Path) -> String {
    fn directory_stats(path: &Path) -> (usize, u64, usize) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return (0, 0, 0);
        };
        let mut entries_count = 0usize;
        let mut file_bytes = 0u64;
        let mut directories = 0usize;
        for entry in entries.flatten() {
            entries_count += 1;
            if let Ok(meta) = entry.metadata() {
                if meta.is_dir() {
                    directories += 1;
                } else if meta.is_file() {
                    file_bytes = file_bytes.saturating_add(meta.len());
                }
            }
        }
        (entries_count, file_bytes, directories)
    }

    let config = dir.join("edge_config.json");
    let wal = dir.join("wal");
    let segments = dir.join("segments");
    let (wal_entries, wal_file_bytes, wal_directories) = directory_stats(&wal);
    let (segment_entries, segment_file_bytes, segment_directories) = directory_stats(&segments);
    let available_bytes = fs2::available_space(dir).ok();
    let readonly = std::fs::metadata(dir)
        .map(|meta| meta.permissions().readonly())
        .ok();
    format!(
        "dir_exists={};config_exists={};config_bytes={};wal_exists={};wal_entries={wal_entries};wal_dirs={wal_directories};wal_file_bytes={wal_file_bytes};segments_exists={};segment_entries={segment_entries};segment_dirs={segment_directories};segment_file_bytes={segment_file_bytes};readonly={};available_bytes={}",
        dir.exists(),
        config.exists(),
        std::fs::metadata(&config).map(|meta| meta.len()).unwrap_or(0),
        wal.is_dir(),
        segments.is_dir(),
        readonly.map(|value| value.to_string()).unwrap_or_else(|| "unknown".into()),
        available_bytes.map(|value| value.to_string()).unwrap_or_else(|| "unknown".into()),
    )
}

/// Open or create the vector index. Mmap vectors and HNSW graphs allow demand paging; int8
/// scalar quantization reduces search and optimization cost.
pub fn open_index(dir: &Path) -> AppResult<EdgeShard> {
    std::fs::create_dir_all(dir)?;
    // Qdrant's atomic writes use Win32 APIs that need extended-length paths
    // for deep segment/temporary directories. Keep this conversion at the IO
    // boundary so library metadata and paths passed to other apps stay familiar.
    #[cfg(target_os = "windows")]
    let io_dir = std::fs::canonicalize(dir)?;
    #[cfg(target_os = "windows")]
    let dir = io_dir.as_path();
    // The placement enum is not re-exported by qdrant-edge; use its public serde value.
    let hnsw = HnswIndexConfig {
        memory: Some(serde_json::from_value(json!("cold"))?),
        ..Default::default()
    };
    let quantization: ScalarQuantizationConfig = serde_json::from_value(json!({"type": "int8"}))?;
    let config = EdgeConfigBuilder::new()
        .vector(
            VECTOR_NAME,
            EdgeVectorParamsBuilder::new(EMBED_DIM, Distance::Cosine)
                .on_disk(true)
                .quantization_config(quantization.into())
                .hnsw_config(hnsw)
                .build(),
        )
        .build();
    if dir.join("edge_config.json").exists() {
        EdgeShard::load(dir, Some(config))
    } else {
        EdgeShard::new(dir, config)
    }
    .map_err(|e| AppError::Msg(format!("vector index open failed: {e}")))
}

/// Lazily open the library's shared shard. Never create a second WAL owner.
pub fn ensure_open(session: &LibrarySession) -> AppResult<()> {
    let mut slot = session.vector.lock().unwrap();
    if slot.is_some() {
        return Ok(());
    }
    if !matches!(
        *session.vector_health.lock().unwrap(),
        crate::library::VectorHealth::Ready
    ) {
        return Err(AppError::coded(
            "vector_index_not_initialized",
            "The vector index needs repair",
        ));
    }
    match open_index(&crate::library::LibraryDirs::new(&session.root).vector()) {
        Ok(shard) => {
            *slot = Some(shard);
            Ok(())
        }
        Err(error) => {
            *session.vector_health.lock().unwrap() = crate::library::VectorHealth::RebuildPending {
                code: "vector_index_open_failed".into(),
                message: error.to_string(),
            };
            Err(error)
        }
    }
}

/// Build an equality filter.
fn cond_match(key: &str, value: ValueVariants) -> Condition {
    Condition::Field(FieldCondition {
        key: key.parse().expect("valid payload field name"),
        r#match: Some(Match::new_value(value)),
        range: None,
        geo_bounding_box: None,
        geo_radius: None,
        geo_polygon: None,
        values_count: None,
        is_empty: None,
        is_null: None,
    })
}

/// Build a numeric-range filter.
fn cond_range(key: &str, gte: Option<f64>, lte: Option<f64>) -> Condition {
    Condition::Field(FieldCondition {
        key: key.parse().expect("valid payload field name"),
        r#match: None,
        range: Some(RangeInterface::Float(Range {
            lt: None,
            gt: None,
            gte: gte.map(OrderedFloat),
            lte: lte.map(OrderedFloat),
        })),
        geo_bounding_box: None,
        geo_radius: None,
        geo_polygon: None,
        values_count: None,
        is_empty: None,
        is_null: None,
    })
}

/// Index-unit payload fields mirror search filters.
fn frame_payload_json(frame: &db::assets::PendingFrame) -> JsonValue {
    let aspect = frame
        .width
        .zip(frame.height)
        .and_then(|(w, h)| (h > 0).then_some(w as f64 / h as f64));
    json!({"seq": frame.seq,"timestamp_ms": frame.timestamp_ms,"asset_id": frame.asset_id,"folder_id": frame.folder_id,"kind": frame.kind,"colors": frame.colors,"tags": frame.tags,"deleted": frame.deleted,"w": frame.width,"h": frame.height,"aspect": aspect,"size": frame.size,"imported_at": frame.imported_at})
}

/// Convert a JSON object into a Qdrant payload.
fn to_payload(v: JsonValue) -> Payload {
    let map: Map<String, JsonValue> = v.as_object().cloned().unwrap_or_default();
    Payload(map)
}

/// Batch-upsert index-unit vectors.
pub fn upsert_points(
    session: &LibrarySession,
    points: Vec<(i64, Vec<f32>, JsonValue)>,
) -> AppResult<()> {
    ensure_open(session)?;
    let shard = session.vector.lock().unwrap();
    let Some(shard) = shard.as_ref() else {
        return Err(AppError::coded(
            "vector_index_not_initialized",
            "vector index is not initialized",
        ));
    };
    upsert_points_to_shard(shard, points)
}

pub(crate) fn upsert_points_to_shard(
    shard: &EdgeShard,
    points: Vec<(i64, Vec<f32>, JsonValue)>,
) -> AppResult<()> {
    let structs: Vec<_> = points
        .into_iter()
        .map(|(id, vec, payload)| {
            // Use the named vector clip rather than an unnamed vector, which Qdrant rejects.
            let vectors: qdrant_edge::Vectors = std::collections::HashMap::from([(
                VECTOR_NAME.to_string(),
                Vector::new_dense(vec),
            )])
            .into();
            PointStruct::new(PointId::NumId(id as u64), vectors, payload)
        })
        .collect::<Vec<PointStruct>>()
        .into_iter()
        .map(Into::into)
        .collect();
    let ops = UpdateOperation::PointOperation(PointOperations::UpsertPoints(
        PointInsertOperations::PointsList(structs),
    ));
    shard
        .update(ops)
        .map_err(|e| AppError::Msg(format!("vector write failed: {e}")))?;
    // Flush vectors before SQLite marks rows embedded so crashes cannot leave committed
    // readiness with memory-only points.
    shard
        .flush()
        .map_err(|e| AppError::Msg(format!("vector flush failed: {e}")))?;
    Ok(())
}

/// Return vector-search hits ordered by similarity.
pub fn search(
    session: &LibrarySession,
    query: Vec<f32>,
    filter: Option<Filter>,
    limit: usize,
) -> AppResult<Vec<qdrant_edge::ScoredPoint>> {
    ensure_open(session)?;
    let shard = session.vector.lock().unwrap();
    let Some(shard) = shard.as_ref() else {
        return Err(AppError::coded(
            "vector_index_not_initialized",
            "vector index is not initialized",
        ));
    };
    let mut req = QueryRequest::new(limit);
    req.query = Some(ScoringQuery::Vector(QueryEnum::Nearest(NamedQuery::new(
        Vector::from(query).into(),
        VECTOR_NAME,
    ))));
    req.filter = filter;
    req.with_payload = WithPayloadInterface::Bool(true);
    shard
        .query(req)
        .map_err(|e| AppError::Msg(format!("vector query failed: {e}")))
}

/// Reconcile SQLite embedded flags with vector points after crashes between independent
/// commits. Reset missing points to pending for startup repair.
pub fn reconcile_embedding_state(session: &LibrarySession) -> AppResult<usize> {
    let embedded_ids: Vec<i64> = {
        let conn = session.db.lock().unwrap();
        let mut stmt = conn.prepare("SELECT fr.id FROM frames fr JOIN assets a ON a.id=fr.asset_id WHERE fr.embedded=1 AND a.visual_indexed=1 ORDER BY fr.id")?;
        let ids = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids
    };
    if embedded_ids.is_empty() {
        return Ok(0);
    }

    let existing: std::collections::HashSet<i64> = {
        let shard = session.vector.lock().unwrap();
        let Some(shard) = shard.as_ref() else {
            return Err(AppError::coded(
                "vector_index_not_initialized",
                "cannot reconcile because vector index is unavailable",
            ));
        };
        let mut ids = std::collections::HashSet::new();
        for chunk in embedded_ids.chunks(256) {
            let points = chunk.iter().map(|id| PointId::NumId(*id as u64)).collect();
            let records = shard
                .retrieve(RetrieveRequest::new(points))
                .map_err(|e| AppError::Msg(format!("vector consistency check failed: {e}")))?;
            for record in records {
                if let PointId::NumId(id) = record.id {
                    ids.insert(id as i64);
                }
            }
        }
        ids
    };

    let missing: Vec<i64> = embedded_ids
        .into_iter()
        .filter(|id| !existing.contains(id))
        .collect();
    if missing.is_empty() {
        return Ok(0);
    }

    let mut conn = session.db.lock().unwrap();
    let tx = conn.transaction()?;
    let mut affected = std::collections::HashSet::new();
    for frame_id in &missing {
        let asset_id: i64 =
            tx.query_row("SELECT asset_id FROM frames WHERE id=?", [frame_id], |r| {
                r.get(0)
            })?;
        tx.execute("UPDATE frames SET embedded=0 WHERE id=?", [frame_id])?;
        affected.insert(asset_id);
    }
    for id in affected {
        tx.execute("UPDATE assets SET visual_indexed=0 WHERE id=?", [id])?;
        db::indexing::enqueue(&tx, id, "visual")?;
    }
    tx.commit()?;
    Ok(missing.len())
}

/// Synchronize one asset's payload after tag, folder, or deletion changes.
pub fn sync_asset_payload(session: &LibrarySession, asset_id: i64) -> AppResult<()> {
    sync_assets_payload(session, &[asset_id])
}

/// Batch-synchronize asset filter payloads.
pub fn sync_assets_payload(session: &LibrarySession, asset_ids: &[i64]) -> AppResult<()> {
    if crate::library::LibraryDirs::new(&session.root)
        .vector()
        .join("edge_config.json")
        .is_file()
    {
        ensure_open(session)?;
    }

    if asset_ids.is_empty() {
        return Ok(());
    }
    for asset_id in asset_ids {
        let (common, frame_ids) = {
            let conn = session.db.lock().unwrap();
            let Some(a) = db::assets::get_asset(&conn, &session.root, *asset_id)? else {
                continue;
            };
            let fids: Vec<i64> = db::assets::get_frame_ids(&conn, *asset_id)?
                .into_iter()
                .map(|(fid, _)| fid)
                .collect();
            if fids.is_empty() {
                continue;
            }
            // SetPayload updates filters, not sequence numbers.
            (
                json!({
                    "folder_id": a.folder_id,
                    "kind": a.kind,
                    "tags": a.tags,
                    "deleted": a.deleted_at.is_some(),
                    "w": a.width,
                    "h": a.height,
                    "aspect": a.width.zip(a.height).and_then(|(w, h)| (h > 0).then_some(w as f64 / h as f64)),
                    "size": a.file_size,
                    "imported_at": a.imported_at,
                }),
                fids,
            )
        };
        for frame_id in frame_ids {
            let colors = {
                let conn = session.db.lock().unwrap();
                db::assets::frame_colors(&conn, frame_id)?
            };
            let mut payload = common.clone();
            payload["colors"] = json!(colors);
            let mut points = vec![PointId::NumId(frame_id as u64)];
            let shard = session.vector.lock().unwrap();
            if let Some(shard) = shard.as_ref() {
                // Update existing points only; pending units have no point and SetPayload
                // would fail.
                if let Ok(records) = shard.retrieve(RetrieveRequest::new(points.clone())) {
                    points = records.iter().map(|r| r.id).collect();
                }
                if points.is_empty() {
                    continue;
                }
                let ops = UpdateOperation::PayloadOperation(PayloadOps::SetPayload(SetPayloadOp {
                    payload: to_payload(payload),
                    points: Some(points),
                    filter: None,
                    key: None,
                }));
                shard
                    .update(ops)
                    .map_err(|e| AppError::Msg(format!("payload sync failed: {e}")))?;
            }
        }
    }
    Ok(())
}

/// Delete vectors by point ID.
pub fn delete_points(session: &LibrarySession, point_ids: &[i64]) -> AppResult<()> {
    if crate::library::LibraryDirs::new(&session.root)
        .vector()
        .join("edge_config.json")
        .is_file()
    {
        ensure_open(session)?;
    }

    if point_ids.is_empty() {
        return Ok(());
    }
    let shard = session.vector.lock().unwrap();
    let Some(shard) = shard.as_ref() else {
        return Ok(());
    };
    let ops = UpdateOperation::PointOperation(PointOperations::DeletePoints {
        ids: point_ids
            .iter()
            .map(|id| PointId::NumId(*id as u64))
            .collect(),
    });
    shard
        .update(ops)
        .map_err(|e| AppError::Msg(format!("vector delete failed: {e}")))?;
    Ok(())
}

/// Delete all points by asset payload ID, including historical orphans missing from SQLite.
pub fn delete_asset_points(session: &LibrarySession, asset_id: i64) -> AppResult<()> {
    if crate::library::LibraryDirs::new(&session.root)
        .vector()
        .join("edge_config.json")
        .is_file()
    {
        let _ = ensure_open(session);
    }

    let shard = session.vector.lock().unwrap();
    let Some(shard) = shard.as_ref() else {
        return Ok(());
    };
    let filter = Filter {
        must: Some(vec![cond_match(
            "asset_id",
            ValueVariants::Integer(asset_id),
        )]),
        ..Default::default()
    };
    let ops = UpdateOperation::PointOperation(PointOperations::DeletePointsByFilter(filter));
    shard
        .update(ops)
        .map_err(|e| AppError::Msg(format!("asset vector cleanup failed: {e}")))?;
    shard
        .flush()
        .map_err(|e| AppError::Msg(format!("asset vector cleanup flush failed: {e}")))?;
    Ok(())
}

/// Optimize once after the batch. Upserts already flush and replacement segments persist
/// before swapping, so another full-segment flush is unnecessary.
pub fn optimize_index(session: &LibrarySession) -> AppResult<()> {
    let shard = session.vector.lock().unwrap();
    let Some(shard) = shard.as_ref() else {
        return Err(AppError::coded(
            "vector_index_not_initialized",
            "cannot optimize because vector index is unavailable",
        ));
    };
    let optimize_started = std::time::Instant::now();
    shard
        .optimize()
        .map_err(|e| AppError::Msg(format!("vector optimize failed: {e}")))?;
    crate::logging::info(
        "import",
        "vector_optimize_done",
        "-",
        &format!("elapsed_ms={}", optimize_started.elapsed().as_millis()),
    );
    Ok(())
}

/// Convert structured filters into Qdrant semantic-search prefilters.
pub fn build_filter(f: &db::model::AssetFilter) -> Option<Filter> {
    let mut must: Vec<Condition> = Vec::new();

    // Exclude trashed assets from search.
    must.push(cond_match("deleted", ValueVariants::Bool(false)));

    if let Some(asset_id) = f.asset_id {
        must.push(cond_match("asset_id", ValueVariants::Integer(asset_id)));
    }

    // Combine conditions with AND.
    if let Some(fid) = f.folder_id {
        must.push(cond_match("folder_id", ValueVariants::Integer(fid)));
    } else if f.uncategorized || !f.folder_ids.is_empty() {
        let mut choices: Vec<Condition> = f
            .folder_ids
            .iter()
            .map(|fid| cond_match("folder_id", ValueVariants::Integer(*fid)))
            .collect();
        if f.uncategorized {
            let path: JsonPath = "folder_id".parse().expect("valid field name");
            choices.push(Condition::IsNull(IsNullCondition::from(path)));
        }
        must.push(Condition::Filter(Filter {
            should: Some(choices),
            ..Default::default()
        }));
    }
    for tag in &f.tags {
        // Each selected tag is an AND condition; each array condition matches any payload
        // element.
        must.push(cond_match("tags", ValueVariants::String(tag.clone())));
    }
    let or_group = |key: &str, values: &[String]| {
        Condition::Filter(Filter {
            should: Some(
                values
                    .iter()
                    .map(|v| cond_match(key, ValueVariants::String(v.clone())))
                    .collect(),
            ),
            ..Default::default()
        })
    };
    if !f.kinds.is_empty() {
        must.push(or_group("kind", &f.kinds));
    }
    if !f.colors.is_empty() {
        must.push(or_group("colors", &f.colors));
    }

    // Numeric ranges.
    let min_res = f.min_width.max(f.min_height).filter(|v| *v > 0);
    if let Some(v) = min_res {
        // Require both width and height thresholds.
        must.push(cond_range("w", Some(v as f64), None));
        must.push(cond_range("h", Some(v as f64), None));
    }
    if f.min_size.is_some() || f.max_size.is_some() {
        must.push(cond_range(
            "size",
            f.min_size.map(|v| v as f64),
            f.max_size.map(|v| v as f64),
        ));
    }
    if f.min_aspect.is_some() || f.max_aspect.is_some() {
        must.push(cond_range("aspect", f.min_aspect, f.max_aspect));
    }
    if f.date_from.is_some() || f.date_to.is_some() {
        must.push(cond_range(
            "imported_at",
            f.date_from.map(|v| v as f64),
            f.date_to.map(|v| v as f64),
        ));
    }

    Some(Filter {
        must: Some(must),
        ..Default::default()
    })
}

#[cfg(test)]
#[path = "../../tests/unit/vector_filter.rs"]
mod filter_tests;
