//! Transport-neutral vector retrieval, SQLite enrichment and ranked pagination.
//! Encoders and transport events remain with their respective entry points.

use std::collections::{BTreeSet, HashMap};

use serde::Serialize;

use crate::db::{
    self,
    model::{Asset, AssetFilter, Paged},
};
use crate::error::AppResult;
use crate::library::LibrarySession;
use crate::vector;

/// Search hit including its similarity score.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    /// Cosine similarity; higher is more relevant.
    pub score: f32,
    pub frame_id: i64,
    /// Stable index-unit UID for external interfaces.
    pub frame_uid: String,
    /// Internal index-unit sequence number.
    pub seq: i64,
    /// Whole-asset index position; zero in the current model.
    pub timestamp_ms: i64,
    /// Absolute preview path; static images may fall back to the asset preview.
    pub frame_path: Option<String>,
    pub asset: Asset,
}

fn searchable_asset_name(asset: &Asset) -> &str {
    asset
        .display_name
        .as_deref()
        .filter(|name| !name.trim().is_empty())
        .unwrap_or(&asset.file_name)
}

/// Maximum recalled candidates before display-title filtering.
const RECALL_LIMIT: usize = 1000;

/// Transport-independent semantic search shared by desktop and MCP.
pub fn run_search_session(
    s: &LibrarySession,
    query: Vec<f32>,
    filter: Option<&AssetFilter>,
    page: Option<u32>,
    page_size: Option<u32>,
) -> AppResult<Paged<SearchHit>> {
    let default_filter = AssetFilter::default();
    let f = filter.unwrap_or(&default_filter);

    // Vector search with structured prefilters.
    let mut qfilter = vector::build_filter(f).unwrap_or_default();
    // Exclude unusable generations before top-k recall, including subtitle-state filters.
    let excluded =
        db::search::excluded_visual_frame_ids(&s.read_db(), f.visual_indexed, f.subtitle_indexed)?;
    if !excluded.is_empty() {
        qfilter
            .must_not
            .get_or_insert_with(Vec::new)
            .push(qdrant_edge::Condition::HasId(
                excluded
                    .into_iter()
                    .map(|id| qdrant_edge::PointId::NumId(id as u64))
                    .collect(),
            ));
    }
    let qfilter = Some(qfilter);
    let hits = vector::search(s, query, qfilter, RECALL_LIMIT)?;

    // Preserve recall and post-filter semantics while enriching candidates in batches.
    let candidates: Vec<_> = hits
        .into_iter()
        .filter_map(|hit| {
            let asset_id = payload_i64(hit.payload.as_ref()?, "asset_id")?;
            let frame_id = match hit.id {
                qdrant_edge::PointId::NumId(id) => id as i64,
                _ => return None,
            };
            (asset_id > 0).then_some((asset_id, frame_id, hit.score))
        })
        .collect();
    let mut out: Vec<SearchHit> = Vec::new();
    {
        let conn = s.read_db();
        let ids: Vec<_> = candidates
            .iter()
            .map(|(id, _, _)| *id)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let assets: HashMap<_, _> = db::assets::get_assets_by_ids(&conn, &s.root, &ids)?
            .into_iter()
            .map(|asset| (asset.id, asset))
            .collect();
        let frame_ids: Vec<_> = candidates.iter().map(|(_, id, _)| *id).collect();
        let mut frames: HashMap<_, _> = db::assets::get_frames_by_ids(&conn, &s.root, &frame_ids)?
            .into_iter()
            .map(|frame| (frame.id, frame))
            .collect();
        let name_query = f
            .query
            .as_deref()
            .map(str::trim)
            .filter(|query| !query.is_empty())
            .map(str::to_lowercase);
        for (asset_id, frame_id, score) in candidates {
            let Some(frame) = frames.remove(&frame_id) else {
                continue;
            };
            let Some(asset) = assets.get(&asset_id) else {
                continue;
            };
            if asset.deleted_at.is_some()
                || !asset.visual_indexed
                || f.visual_indexed == Some(false)
                || f.subtitle_indexed
                    .is_some_and(|v| v != asset.subtitle_indexed)
            {
                continue;
            }
            if name_query
                .as_ref()
                .is_some_and(|query| !searchable_asset_name(asset).to_lowercase().contains(query))
            {
                continue;
            }
            out.push(SearchHit {
                score,
                frame_id,
                frame_uid: frame.uid,
                seq: frame.seq,
                timestamp_ms: frame.timestamp_ms,
                frame_path: frame.abs_path,
                asset: asset.clone(),
            });
        }
    }

    // Sort explicitly by descending relevance and frame_id as a stable tie-breaker across
    // searches and pages.
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.frame_id.cmp(&b.frame_id))
    });

    // Paginate results.
    let page = page.unwrap_or(1).max(1) as usize;
    let page_size = page_size.unwrap_or(40).clamp(1, 200) as usize;
    let total = out.len() as i64;
    let has_more = page * page_size < out.len();
    let items = out
        .into_iter()
        .skip((page - 1) * page_size)
        .take(page_size)
        .collect();
    Ok(Paged {
        items,
        total,
        page: page as u32,
        page_size: page_size as u32,
        has_more,
    })
}

/// Read an integer payload field.
fn payload_i64(payload: &qdrant_edge::Payload, key: &str) -> Option<i64> {
    payload.0.get(key).and_then(|v| v.as_i64())
}

#[cfg(test)]
#[path = "../../tests/unit/search.rs"]
mod tests;
