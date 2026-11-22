use serde::{Deserialize, Serialize};

/// Asset DTO for presentation; resolve library-relative paths against the root.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Asset {
    pub id: i64,
    pub uid: String,
    pub folder_id: Option<i64>,
    pub folder_name: Option<String>,
    pub file_name: String,
    /// Optional display title; fall back to file_name when empty.
    pub display_name: Option<String>,
    /// Library-relative path using forward slashes.
    pub rel_path: String,
    /// Absolute display/playback path; references resolve to their source and legacy managed
    /// assets to their library copy.
    pub abs_path: String,
    /// Storage kind: reference for new assets; copy remains for legacy database
    /// compatibility.
    pub storage: &'static str,
    /// Absolute reference source path; None for legacy managed assets.
    pub source_path: Option<String>,
    /// Reference source modification time at registration, in Unix seconds.
    pub source_mtime: Option<i64>,
    /// Whether a reference source has been removed or moved, maintained by checks and
    /// operations.
    pub source_missing: bool,
    /// Legacy web provenance; ordinary local imports leave it unset.
    pub web_source_url: Option<String>,
    pub web_source_title: Option<String>,
    /// Downloaded video identity and chosen media format; older link records remain readable.
    pub file_size: i64,
    pub mime_type: String,
    /// image | video | gif
    pub kind: String,
    pub width: Option<i64>,
    pub height: Option<i64>,
    pub duration_ms: Option<i64>,
    /// Dominant color among twelve named buckets.
    pub color: Option<String>,
    pub imported_at: i64,
    /// Trash timestamp; None means active.
    pub deleted_at: Option<i64>,
    /// Import stage: done, pending, or error:<reason>.
    pub import_stage: String,
    pub processing_tasks: Vec<super::processing::Task>,
    pub visual_indexed: bool,
    pub subtitle_indexed: bool,
    pub subtitle_available: bool,
    pub tags: Vec<String>,
    /// Absolute thumbnail path, falling back to abs_path when unavailable.
    pub thumb_path: Option<String>,
}

/// Asset details, including internal index units.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetDetail {
    #[serde(flatten)]
    pub asset: Asset,
    /// Absolute internal preview paths ordered by sequence; empty for static images.
    pub frames: Vec<FrameInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FrameInfo {
    pub id: i64,
    pub uid: String,
    pub seq: i64,
    pub timestamp_ms: i64,
    pub abs_path: Option<String>,
}

/// Optional asset-list filters.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct AssetFilter {
    /// Restrict to one library-local asset ID.
    pub asset_id: Option<i64>,
    /// Case-insensitive filename matching.
    pub query: Option<String>,
    /// Restrict to a folder.
    pub folder_id: Option<i64>,
    /// Multiple folders OR with uncategorized; a fixed folder_id takes precedence.
    pub folder_ids: Vec<i64>,
    /// Uncategorized means folder_id IS NULL.
    pub uncategorized: bool,
    /// Require all selected tags.
    pub tags: Vec<String>,
    /// Media kind: image, video, or gif.
    pub kinds: Vec<String>,
    /// Dominant-color bucket.
    pub colors: Vec<String>,
    pub min_width: Option<i64>,
    pub min_height: Option<i64>,
    /// Aspect-ratio bounds, width divided by height.
    pub min_aspect: Option<f64>,
    pub max_aspect: Option<f64>,
    pub min_size: Option<i64>,
    pub max_size: Option<i64>,
    /// Import-date bounds in Unix seconds.
    pub date_from: Option<i64>,
    pub date_to: Option<i64>,
    /// Query trashed assets.
    pub in_trash: bool,
    /// Only failed imports whose stage begins with error.
    pub failed_only: bool,
    /// Only references with missing source files.
    pub source_missing: bool,
    pub visual_indexed: Option<bool>,
    pub subtitle_indexed: Option<bool>,
    pub page: u32,
    pub page_size: u32,
    /// Keyset cursor for the default imported_desc order.
    pub cursor_imported_at: Option<i64>,
    pub cursor_id: Option<i64>,
    /// Ordering: imported_desc by default, imported_asc, name_asc, name_desc, or size_desc.
    pub sort: Option<String>,
}

/// Paginated result.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Paged<T> {
    pub items: Vec<T>,
    pub total: i64,
    pub page: u32,
    pub page_size: u32,
    pub has_more: bool,
}

/// Folder DTO including live asset count.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: i64,
    pub name: String,
    pub created_at: i64,
    pub asset_count: i64,
}

/// Tag DTO including reference count.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub id: i64,
    pub name: String,
    pub asset_count: i64,
    pub created_at: i64,
}
