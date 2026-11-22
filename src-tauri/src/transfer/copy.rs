//! File, row and vector copying; source reads and destination writes defer lifecycle cleanup.
use super::{duplicates::merge_duplicate_metadata, CopyAssetOutcome};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession};
use crate::{util, vector};
use qdrant_edge::{
    PointId, RetrieveRequest, VectorStructInternal, WithPayloadInterface, WithVector,
};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Copy complete asset metadata, managed files, tags, and vectors; folder_id_in_dst selects
/// the logical destination and the result is its local asset ID.
pub(super) fn copy_asset_to_session_outcome(
    src: &LibrarySession,
    asset_id: i64,
    dst: &LibrarySession,
    folder_id_in_dst: Option<i64>,
) -> AppResult<CopyAssetOutcome> {
    let _source_work = src.begin_asset_work(asset_id)?;
    // Read the source asset.
    let (asset, src_folder_id, source_hash, cover_manual) = {
        let conn = src.db.lock().unwrap();
        let asset = db::assets::get_asset(&conn, &src.root, asset_id)?
            .ok_or_else(|| AppError::AssetNotFound)?;
        let (fid, hash): (Option<i64>, String) = conn.query_row(
            "SELECT folder_id, file_hash FROM assets WHERE id = ?1",
            rusqlite::params![asset_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let cover_manual = db::assets::has_manual_cover(&conn, asset_id)?;
        (asset, fid, hash, cover_manual)
    };
    if asset.deleted_at.is_some() {
        return Err(AppError::coded(
            "asset_in_trash",
            "asset is in the trash; restore it before moving",
        ));
    }
    // Use UID as retry identity; reuse complete targets and clean incomplete targets before
    // retry.
    let existing_uid = {
        let conn = dst.db.lock().unwrap();
        db::assets::find_by_uid(&conn, &asset.uid)?
    };
    if let Some((id, hash)) = existing_uid {
        if hash != source_hash {
            return Err(AppError::Msg(format!(
                "Asset UID conflicts with different content: {}",
                asset.uid
            )));
        }
        let complete = super::duplicates::reusable_target(dst, id, src, &asset)?;
        if complete {
            let _target_work = dst.asset_activity.work()?;
            merge_duplicate_metadata(dst, id, &asset, folder_id_in_dst)?;

            return Ok(CopyAssetOutcome::Existing(id));
        }
        crate::services::assets::delete_permanent(dst, &[id])?;
    } else {
        let conn = dst.db.lock().unwrap();
        if let Some(existing_id) = db::assets::find_by_hash(&conn, &source_hash)? {
            drop(conn);
            if !super::duplicates::reusable_target(dst, existing_id, src, &asset)? {
                return Err(AppError::coded(
                    "library_file_missing",
                    "The duplicate target is incomplete; source has been retained",
                ));
            }
            let _target_work = dst.asset_activity.work()?;
            merge_duplicate_metadata(dst, existing_id, &asset, folder_id_in_dst)?;

            return Ok(CopyAssetOutcome::Existing(existing_id));
        }
        drop(conn);
    }

    let _target_work = dst.asset_activity.work()?;
    let dst_dirs = LibraryDirs::new(&dst.root);
    let orphan = dst_dirs.asset(asset.imported_at, &asset.uid);
    if orphan.exists() {
        fs::remove_dir_all(orphan)?;
    }

    // Validate destination membership for both references and legacy managed assets.
    if let Some(fid) = folder_id_in_dst {
        let conn = dst.db.lock().unwrap();
        db::folders::get_name(&conn, fid)?
            .ok_or_else(|| AppError::coded("folder_not_found", "target folder does not exist"))?;
    }
    let staging = dst_dirs.app().join("importing");
    fs::create_dir_all(&staging)?;
    let new_file_name = asset.file_name.clone();
    // Recompute hashes to respect destination uniqueness and detect source changes.
    let file_hash = source_hash;
    let file_size = if asset.storage == "reference" {
        asset.file_size
    } else {
        fs::metadata(&asset.abs_path)?.len() as i64
    };

    // Copy bytes outside the writer mutex; a pending row records the move boundary.
    let asset_dir = dst_dirs.asset(asset.imported_at, &asset.uid);
    fs::create_dir_all(&asset_dir)?;
    let final_dest = asset_dir.join(&new_file_name);
    let final_rel = util::to_rel(&dst.root, &final_dest);
    let staged = if asset.storage == "copy" {
        let path = crate::services::assets::unique_target(&staging, &new_file_name);
        fs::copy(&asset.abs_path, &path)?;
        Some(path)
    } else {
        None
    };
    let staging_rel = staged.as_ref().map(|path| util::to_rel(&dst.root, path));
    let new_id = {
        let conn = dst.db.lock().unwrap();
        let id = db::assets::insert_asset(
            &conn,
            db::assets::NewAsset {
                folder_id: folder_id_in_dst,
                file_name: &new_file_name,
                rel_path: staging_rel.as_deref().unwrap_or(&final_rel),
                file_hash: &file_hash,
                file_size,
                mime_type: &asset.mime_type,
                kind: &asset.kind,
                imported_at: asset.imported_at,
                uid: Some(&asset.uid),
                source: asset
                    .source_path
                    .as_deref()
                    .map(|path| db::assets::ReferenceSource {
                        path,
                        modified_at: asset.source_mtime.unwrap_or(asset.imported_at),
                    }),
            },
        )?;

        if asset.storage == "reference" {
            db::assets::set_source_missing(&conn, id, asset.source_missing)?;
        }
        id
    };
    if let Some(staged) = staged {
        crate::services::assets::move_file(staged.to_string_lossy().as_ref(), &final_dest)?;
        db::assets::set_asset_rel_path(
            &dst.db.lock().unwrap(),
            new_id,
            &final_rel,
            &new_file_name,
        )?;
    }
    let new_thumb_rel = asset
        .thumb_path
        .as_ref()
        .map(|path| -> AppResult<String> {
            let to = dst_dirs.thumbnail(asset.imported_at, &asset.uid, &new_file_name);
            fs::copy(path, &to)?;
            Ok(util::to_rel(&dst.root, &to))
        })
        .transpose()?;
    {
        let conn = dst.db.lock().unwrap();
        let id = new_id;
        let thumb_rel = &new_thumb_rel;
        db::assets::update_asset_probe(
            &conn,
            id,
            crate::db::assets::AssetProbe {
                width: asset.width,
                height: asset.height,
                duration_ms: asset.duration_ms,
                color: asset.color.as_deref(),
                thumb_rel: thumb_rel.as_deref(),
            },
        )?;
        if !asset.tags.is_empty() {
            db::tags::add_to_asset(&conn, id, &asset.tags)?;
        }
        if let Some(url) = asset.web_source_url.as_deref() {
            db::assets::set_web_source(
                &conn,
                id,
                url,
                asset.web_source_title.as_deref().unwrap_or_default(),
            )?;
        }
        if asset.display_name.is_some() {
            db::assets::set_display_name(&conn, id, asset.display_name.as_deref())?;
        }
        if cover_manual {
            conn.execute("UPDATE assets SET cover_manual=1 WHERE id=?1", [id])?;
        }
    }
    // Proxies belong to the asset, including reference assets. Never copy or
    // move the external original here; failures only discard this optional cache.
    let source_directory = LibraryDirs::new(&src.root).asset(asset.imported_at, &asset.uid);
    let target_directory = dst_dirs.asset(asset.imported_at, &asset.uid);
    let target_source = if asset.storage == "reference" {
        Path::new(&asset.abs_path)
    } else {
        final_dest.as_path()
    };
    if let Err(error) = crate::media::raw_video::copy_cached_proxy(
        Path::new(&asset.abs_path),
        &source_directory,
        target_source,
        &target_directory,
    ) {
        crate::logging::error("transfer", "proxy_copy_failed", "-", &error.to_string());
    }
    let _ = src_folder_id; // The caller maps source folders.

    // Regenerate asset colors from the copied cover independently of legacy v9 backfill
    // state.
    {
        let cover = new_thumb_rel
            .as_ref()
            .map(|p| dst.root.join(p))
            .unwrap_or_else(|| final_dest.clone());
        if cover.is_file() {
            let rgb = crate::import::thumb::open_image_smart(&cover)?.to_rgb8();
            let colors =
                crate::import::pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
            let refs: Vec<_> = colors.iter().map(|c| (c.color, c.ratio)).collect();
            let conn = dst.db.lock().unwrap();
            db::assets::replace_asset_colors(&conn, new_id, &refs)?;
            db::assets::set_asset_primary_color(&conn, new_id, colors[0].color)?;
        }
        // If no processed cover exists, retain the copied color field and skip regeneration.
    }

    // Rebuild internal unit rows, derived files, and vectors.
    let src_frames = {
        let conn = src.db.lock().unwrap();
        db::assets::get_frames_for_transfer(&conn, asset_id)?
    };
    // Legacy GIF thumbnails must be re-embedded rather than reusing an old keyframe vector.
    let old_ids: Vec<i64> = if asset.kind == "gif" {
        Vec::new()
    } else {
        src_frames.iter().map(|(fid, _, _, _, _)| *fid).collect()
    };
    let src_vectors = retrieve_vectors(src, &old_ids);

    let mut points: Vec<(i64, Vec<f32>, serde_json::Value)> = Vec::new();
    let mut missing: Vec<i64> = Vec::new();
    if asset.kind == "gif" {
        let frame_uid = src_frames
            .first()
            .map(|frame| frame.1.clone())
            .unwrap_or_else(util::new_entity_uid);
        let frame_path = new_thumb_rel
            .as_ref()
            .map(|path| dst.root.join(path))
            .unwrap_or_else(|| final_dest.clone());
        let rgb = crate::import::thumb::open_image_smart(&frame_path)?.to_rgb8();
        let colors =
            crate::import::pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
        let refs: Vec<_> = colors
            .iter()
            .map(|color| (color.color, color.ratio))
            .collect();
        let conn = dst.db.lock().unwrap();
        let new_fid = db::assets::insert_frame_with_uid(
            &conn,
            &frame_uid,
            new_id,
            0,
            0,
            new_thumb_rel.as_deref(),
        )?;
        db::assets::replace_frame_colors(&conn, new_fid, &refs)?;
        missing.push(new_fid);
    } else {
        for (old_fid, frame_uid, seq, timestamp_ms, old_rel) in &src_frames {
            let new_rel = old_rel
                .as_ref()
                .map(|rel| -> AppResult<String> {
                    let to = dst_dirs
                        .asset_keyframes(asset.imported_at, &asset.uid)
                        .join(format!("{new_id}_{timestamp_ms}.webp"));
                    fs::create_dir_all(to.parent().unwrap())?;
                    fs::copy(src.root.join(rel), &to)?;
                    Ok(util::to_rel(&dst.root, &to))
                })
                .transpose()?;
            let frame_path = new_rel
                .as_ref()
                .map(|path| dst.root.join(path))
                .or_else(|| new_thumb_rel.as_ref().map(|path| dst.root.join(path)))
                .unwrap_or_else(|| final_dest.clone());
            let rgb = crate::import::thumb::open_image_smart(&frame_path)?.to_rgb8();
            let colors =
                crate::import::pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
            let refs: Vec<_> = colors
                .iter()
                .map(|color| (color.color, color.ratio))
                .collect();
            let new_fid = {
                let conn = dst.db.lock().unwrap();
                let id = db::assets::insert_frame_with_uid(
                    &conn,
                    frame_uid,
                    new_id,
                    *seq,
                    *timestamp_ms,
                    new_rel.as_deref(),
                )?;
                db::assets::replace_frame_colors(&conn, id, &refs)?;
                id
            };
            match src_vectors.get(old_fid) {
                Some(v)=> points.push((new_fid,v.clone(),serde_json::json!({
                    "seq":seq,"timestamp_ms":timestamp_ms,"asset_id":new_id,"folder_id":folder_id_in_dst,
                    "kind":asset.kind,"colors":colors.iter().map(|color|color.color).collect::<Vec<_>>(),"tags":asset.tags,
                    "deleted":false,"w":asset.width,"h":asset.height,
                    "aspect":asset.width.zip(asset.height).and_then(|(w,h)|(h>0).then_some(w as f64/h as f64)),
                    "size":file_size,"imported_at":asset.imported_at,
                }))),
                None=>missing.push(new_fid),
            }
        }
    }
    if !points.is_empty() {
        vector::upsert_points(dst, points)?;
    }
    // Subtitle source bytes and timestamps travel with the managed library.

    // Publish the completed state.
    {
        let conn = dst.db.lock().unwrap();
        let source_policy = {
            let source = src.read_db();
            if crate::db::indexing::has_column(&source, "assets", "visual_requested") {
                crate::db::indexing::policy(&source, asset_id)?
            } else {
                crate::db::indexing::IndexPolicy::default()
            }
        };
        crate::db::indexing::set_policy(&conn, new_id, source_policy)?;
        {
            let source = src.read_db();
            if crate::db::indexing::has_column(&source, "index_versions", "recipe") {
                let mut q=source.prepare("SELECT kind,recipe,source_size,source_mtime FROM index_versions WHERE asset_id=?")?;
                let rows = q
                    .query_map([asset_id], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, i64>(2)?,
                            r.get::<_, String>(3)?,
                        ))
                    })?
                    .collect::<Result<Vec<_>, _>>()?;
                for (kind, recipe, size, mtime) in rows {
                    conn.execute("INSERT OR REPLACE INTO index_versions(asset_id,kind,recipe,source_size,source_mtime) VALUES(?1,?2,?3,?4,?5)",rusqlite::params![new_id,kind,recipe,size,mtime])?;
                }
            }
        }
        conn.execute(
            "UPDATE assets SET visual_indexed=?1 WHERE id=?2",
            rusqlite::params![asset.visual_indexed && missing.is_empty(), new_id],
        )?;
        if asset.visual_indexed && !missing.is_empty() {
            crate::db::indexing::enqueue(&conn, new_id, "visual")?;
        }
        db::assets::set_import_stage(
            &conn,
            new_id,
            if missing.is_empty() {
                "done"
            } else {
                "embedding"
            },
        )?;
    }
    Ok(CopyAssetOutcome::Copied(new_id))
}

/// Copy by stable identity and merge existing metadata for safe retries. Read source dense
/// vectors in batches keyed by frame ID.
fn retrieve_vectors(session: &LibrarySession, frame_ids: &[i64]) -> HashMap<i64, Vec<f32>> {
    if !frame_ids.is_empty() {
        let _ = vector::ensure_open(session);
    }
    let mut out = HashMap::new();
    let shard = session.vector.lock().unwrap();
    let Some(shard) = shard.as_ref() else {
        return out;
    };
    let ids: Vec<PointId> = frame_ids
        .iter()
        .map(|id| PointId::NumId(*id as u64))
        .collect();
    let mut req = RetrieveRequest::new(ids);
    req.with_vector = Some(WithVector::Bool(true));
    req.with_payload = Some(WithPayloadInterface::Bool(false));
    let Ok(records) = shard.retrieve(req) else {
        return out;
    };
    for r in records {
        let named = match r.vector {
            Some(VectorStructInternal::Named(named)) => named,
            _ => continue,
        };
        let dense = match named.get(vector::VECTOR_NAME) {
            Some(qdrant_edge::VectorInternal::Dense(d)) => d.clone(),
            _ => continue,
        };
        if let PointId::NumId(id) = r.id {
            out.insert(id as i64, dense);
        }
    }
    out
}
