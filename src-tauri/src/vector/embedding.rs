use super::{frame_payload_json, upsert_points};
use crate::clip::ClipEncoder;
use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::LibrarySession;
use std::sync::Arc;
/// Read and persist this many pending units per batch; model execution may use smaller static
/// batches.
const EMBED_BATCH: i64 = 64;

/// Encode pending units and persist vectors on a background thread. Invoke on_asset after
/// completion and return the number of units processed.
pub fn embed_pending(
    session: &Arc<LibrarySession>,
    clip: &ClipEncoder,
    on_asset: &dyn Fn(&crate::db::model::Asset),
) -> AppResult<usize> {
    embed_pending_while(session, clip, on_asset, &|| true)
}

pub fn embed_pending_while(
    session: &Arc<LibrarySession>,
    clip: &ClipEncoder,
    on_asset: &dyn Fn(&crate::db::model::Asset),
    should_continue: &dyn Fn() -> bool,
) -> AppResult<usize> {
    let _work = session.asset_activity.work()?;
    let mut total = 0usize;
    let mut decode_ms = 0_u128;
    let mut encode_ms = 0_u128;
    let mut persist_ms = 0_u128;
    loop {
        if !should_continue() {
            return Err(AppError::coded("index_cancelled", "Index task cancelled"));
        }

        let rows = {
            let conn = session.db.lock().unwrap();
            db::assets::pending_frames_full(&conn, &session.root, EMBED_BATCH)?
        };
        if rows.is_empty() {
            break;
        }
        // Prepare inputs outside database locks before model execution.
        let started = std::time::Instant::now();

        decode_ms += started.elapsed().as_millis();
        let started = std::time::Instant::now();
        let feats = rows
            .iter()
            .map(|row| {
                let asset = db::assets::get_asset(&session.read_db(), &session.root, row.asset_id)?
                    .ok_or(AppError::AssetNotFound)?;
                clip.encode_asset(
                    std::path::Path::new(&asset.abs_path),
                    asset.kind == "video",
                    &|| !should_continue(),
                )
            })
            .collect::<AppResult<Vec<_>>>()?;
        encode_ms += started.elapsed().as_millis();

        let mut points = Vec::with_capacity(rows.len());
        for (r, feat) in rows.iter().zip(feats.iter()) {
            points.push((r.frame_id, feat.clone(), frame_payload_json(r)));
        }
        let started = std::time::Instant::now();
        // Write and flush one batch before marking SQLite rows embedded.
        if !should_continue() {
            return Err(AppError::coded("index_cancelled", "Index task cancelled"));
        }
        upsert_points(session, points)?;

        persist_embedded_frames(session, &rows)?;
        // Emit completed state outside the database lock.
        let changed = {
            let conn = session.db.lock().unwrap();
            rows.iter()
                .map(|row| db::assets::get_asset(&conn, &session.root, row.asset_id))
                .collect::<AppResult<Vec<_>>>()?
        };
        for asset in changed.into_iter().flatten() {
            if asset.deleted_at.is_none() {
                on_asset(&asset);
            }
        }
        persist_ms += started.elapsed().as_millis();
        total += rows.len();
    }
    if total > 0 {
        crate::logging::info(
            "import",
            "vector_embed_timing",
            "-",
            &format!(
                "frames={total};decode_ms={decode_ms};encode_ms={encode_ms};persist_ms={persist_ms}"
            ),
        );
    }
    Ok(total)
}

fn persist_embedded_frames(
    session: &LibrarySession,
    rows: &[db::assets::PendingFrame],
) -> AppResult<()> {
    let mut conn = session.db.lock().unwrap();
    let tx = conn.transaction()?;
    for r in rows {
        // The batch may have been encoded before deletion intent committed.
        // Publish readiness once below, after marking only still-active frames.
        tx.execute(
            "UPDATE frames SET embedded=1 WHERE id=?1 AND EXISTS(SELECT 1 FROM assets a WHERE a.id=frames.asset_id AND a.deleted_at IS NULL)",
            [r.frame_id],
        )?;
    }
    tx.execute("UPDATE assets SET visual_indexed=1 WHERE deleted_at IS NULL AND EXISTS(SELECT 1 FROM frames f WHERE f.asset_id=assets.id) AND NOT EXISTS(SELECT 1 FROM frames f WHERE f.asset_id=assets.id AND f.embedded=0) AND COALESCE((SELECT t.status FROM index_tasks t WHERE t.asset_id=assets.id AND t.kind='visual' ORDER BY t.id DESC LIMIT 1),'queued') NOT IN ('running','cancelled','failed')",[])?;
    // Mark assets done after all their units have been embedded.
    let done_assets: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT DISTINCT a.id FROM assets a \
             WHERE a.import_stage = 'embedding' AND a.deleted_at IS NULL \
             AND NOT EXISTS (SELECT 1 FROM frames f WHERE f.asset_id = a.id AND f.embedded = 0)",
        )?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<Result<Vec<_>, _>>()?;
        ids
    };
    for id in done_assets {
        db::assets::set_import_stage(&tx, id, "done")?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/vector_embedding.rs"]
mod tests;
