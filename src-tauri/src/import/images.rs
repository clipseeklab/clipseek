//! Static image processing and representative-color maintenance.

use super::{pixels, thumb};
use crate::db;
use crate::error::{AppError, AppResult};
use crate::library::{LibraryDirs, LibrarySession};
use crate::util;
use std::{path::PathBuf, sync::Arc};

/// Probe a static image, generate its preview and colors, and prepare its sequence-zero index
/// unit.
pub(super) fn thumbnail_path(
    session: &LibrarySession,
    dirs: &LibraryDirs,
    asset_id: i64,
    source: &std::path::Path,
) -> AppResult<PathBuf> {
    let (imported_at, asset_uid) = {
        let conn = session.db.lock().unwrap();
        db::assets::get_asset(&conn, &session.root, asset_id)?
            .map(|a| (a.imported_at, a.uid))
            .ok_or_else(|| AppError::AssetNotFound)?
    };
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    Ok(dirs.thumbnail(imported_at, &asset_uid, &name))
}

pub(super) fn import_image(
    session: &Arc<LibrarySession>,
    dirs: &LibraryDirs,
    asset_id: i64,
    file: &std::path::Path,
    visual: bool,
) -> AppResult<()> {
    let rasterized = thumb::needs_rasterized_preview(file);
    let decoded = if rasterized {
        Some(thumb::open_image_smart(file)?)
    } else {
        None
    };
    let (width, height) = decoded
        .as_ref()
        .map(|img| (img.width(), img.height()))
        .map_or_else(|| thumb::image_dimensions(file), Ok)?;
    let (thumb_rel, colors) = if rasterized {
        let thumb_path = thumbnail_path(session, dirs, asset_id, file)?;
        let img = decoded.unwrap();
        let preview = if width.max(height) > thumb::THUMB_MAX {
            img.resize(
                thumb::THUMB_MAX,
                thumb::THUMB_MAX,
                image::imageops::FilterType::CatmullRom,
            )
        } else {
            img
        };
        thumb::encode_dynamic(&thumb_path, &preview)?;
        let rgb = thumb::preview_rgb(&preview);
        let colors = pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
        (Some(util::to_rel(&session.root, &thumb_path)), colors)
    } else {
        let thumb_path = thumbnail_path(session, dirs, asset_id, file)?;
        let img = thumb::open_image_smart(file)?.thumbnail(thumb::THUMB_MAX, thumb::THUMB_MAX);
        thumb::encode_dynamic(&thumb_path, &img)?;
        let rgb = thumb::open_image_smart(&thumb_path)?.to_rgb8();
        let colors = pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
        (Some(util::to_rel(&session.root, &thumb_path)), colors)
    };
    {
        let conn = session.db.lock().unwrap();
        let primary = colors.first().map(|c| c.color).unwrap_or("gray");
        db::assets::update_asset_probe(
            &conn,
            asset_id,
            crate::db::assets::AssetProbe {
                width: Some(width as i64),
                height: Some(height as i64),
                duration_ms: None,
                color: Some(primary),
                thumb_rel: thumb_rel.as_deref(),
            },
        )?;
        let refs: Vec<_> = colors.iter().map(|c| (c.color, c.ratio)).collect();
        db::assets::replace_asset_colors(&conn, asset_id, &refs)?;
        if !visual {
            return Ok(());
        }
        // Static images have one index unit; missing relative input paths fall back to the
        // thumbnail or original.
        let frame_id = db::assets::insert_frame(&conn, asset_id, 0, 0, None)?;
        db::assets::replace_frame_colors(&conn, frame_id, &refs)?;
    }
    Ok(())
}

/// Backfill colors for pre-v9 libraries using existing files without rebuilding media or
/// embeddings; return changed IDs for payload synchronization.
pub fn backfill_color_features(session: &LibrarySession) -> AppResult<Vec<i64>> {
    let assets: Vec<(i64, PathBuf)> = {
        let conn = session.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT a.id, COALESCE(a.thumb_rel, a.rel_path) FROM assets a \
             WHERE NOT EXISTS (SELECT 1 FROM asset_colors ac WHERE ac.asset_id = a.id)",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, session.root.join(r.get::<_, String>(1)?)))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    let mut changed = Vec::new();
    for (asset_id, path) in assets {
        let rgb = match thumb::open_image_smart(&path) {
            Ok(image) => image.to_rgb8(),
            Err(e) => {
                crate::logging::warn(
                    "maintenance",
                    "cover_color_failed",
                    "-",
                    &format!("path=<library>;message={e}"),
                );
                continue;
            }
        };
        let colors = pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
        let refs: Vec<_> = colors.iter().map(|c| (c.color, c.ratio)).collect();
        let conn = session.db.lock().unwrap();
        db::assets::replace_asset_colors(&conn, asset_id, &refs)?;
        db::assets::set_asset_primary_color(&conn, asset_id, colors[0].color)?;
        changed.push(asset_id);
    }

    let frames: Vec<(i64, i64, PathBuf)> = {
        let conn = session.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT fr.id, fr.asset_id, COALESCE(fr.rel_path, a.thumb_rel, a.rel_path) \
             FROM frames fr JOIN assets a ON a.id = fr.asset_id \
             WHERE NOT EXISTS (SELECT 1 FROM frame_colors fc WHERE fc.frame_id = fr.id)",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    session.root.join(r.get::<_, String>(2)?),
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows
    };
    for (frame_id, asset_id, path) in frames {
        let rgb = match thumb::open_image_smart(&path) {
            Ok(image) => image.to_rgb8(),
            Err(e) => {
                crate::logging::warn(
                    "maintenance",
                    "frame_color_failed",
                    "-",
                    &format!("path=<library>;message={e}"),
                );
                continue;
            }
        };
        let colors = pixels::dominant_colors(rgb.width(), rgb.height(), rgb.as_raw());
        let refs: Vec<_> = colors.iter().map(|c| (c.color, c.ratio)).collect();
        let conn = session.db.lock().unwrap();
        db::assets::replace_frame_colors(&conn, frame_id, &refs)?;
        if !changed.contains(&asset_id) {
            changed.push(asset_id);
        }
    }
    Ok(changed)
}
