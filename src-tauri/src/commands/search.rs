//! Semantic text and image search: encode the query, apply Qdrant prefilters, load asset
//! details from SQLite, apply display-title filtering, and paginate by relevance.

use tauri::Emitter;

use crate::db::model::{AssetFilter, Paged};
use crate::error::{AppError, AppResult};

use crate::services::search::{run_search_session, SearchHit};

#[cfg(test)]
#[path = "../../tests/unit/search_preview.rs"]
mod tests;

/// Browser-compatible thumbnail for an image-search query, including HEIC/HEIF.
#[tauri::command]
pub async fn search_image_preview<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    image_path: String,
) -> AppResult<Vec<u8>> {
    crate::commands::blocking(app, move |_| {
        query_image_preview(std::path::Path::new(&image_path))
    })
    .await
}

pub fn query_image_preview(path: &std::path::Path) -> AppResult<Vec<u8>> {
    use crate::import::scanner::{self, Kind};
    if !matches!(scanner::classify(path), Some(Kind::Image | Kind::Gif)) {
        return Err(AppError::coded(
            "preview_type_unsupported",
            "Search preview requires an image",
        ));
    }
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::coded(
            "preview_type_unsupported",
            "Search preview requires a regular file",
        ));
    }
    crate::preview::check_size(metadata.len())?;
    let image = crate::import::thumb::open_image_smart(path)?.thumbnail(128, 128);
    let mut output = std::io::Cursor::new(Vec::new());
    image
        .write_to(&mut output, image::ImageFormat::Png)
        .map_err(|error| AppError::Msg(format!("Search preview encode failed: {error}")))?;
    Ok(output.into_inner())
}

/// Semantic text search.
#[tauri::command]
pub async fn search_text<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    query: String,
    filter: Option<AssetFilter>,
    page: Option<u32>,
    page_size: Option<u32>,
    context: crate::commands::body::LibraryContext,
) -> AppResult<Paged<SearchHit>> {
    if query.trim().is_empty() {
        return Ok(Paged {
            items: vec![],
            total: 0,
            page: 1,
            page_size: 0,
            has_more: false,
        });
    }
    let events = app.clone();
    crate::commands::blocking_in_library(app, context, move |state, session| {
        let query = query.trim();
        let start = std::time::Instant::now();
        let loaded = start.elapsed().as_millis();
        let q = state.clip.encode_text_cached(query)?;
        let encoded = start.elapsed().as_millis();
        let result = run_search_session(session, q, filter.as_ref(), page, page_size);
        if result.is_err() {
            let _ = events.emit("vector://status", crate::vector::recovery::status(session));
        }
        crate::logging::info(
            "search",
            "text_timing",
            "-",
            &format!(
                "init_ms={loaded};encode_ms={};retrieve_ms={};total_ms={}",
                encoded - loaded,
                start.elapsed().as_millis() - encoded,
                start.elapsed().as_millis()
            ),
        );
        result
    })
    .await
}

/// Image search accepts an absolute path selected or dropped by the user.
#[tauri::command]
pub async fn search_image<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    image_path: String,
    filter: Option<AssetFilter>,
    page: Option<u32>,
    page_size: Option<u32>,
    context: crate::commands::body::LibraryContext,
) -> AppResult<Paged<SearchHit>> {
    let events = app.clone();
    crate::commands::blocking_in_library(app, context, move |state, session| {
        let clip = state.clip.get_or_init()?;
        let img = crate::import::thumb::open_image_smart(std::path::Path::new(&image_path))?;
        let q = clip
            .encode_images(&[img])?
            .into_iter()
            .next()
            .ok_or_else(|| AppError::Msg("empty encode result".into()))?;
        let result = run_search_session(session, q, filter.as_ref(), page, page_size);
        if result.is_err() {
            let _ = events.emit("vector://status", crate::vector::recovery::status(session));
        }
        result
    })
    .await
}
