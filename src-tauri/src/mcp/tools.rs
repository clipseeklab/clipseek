//! MCP argument schemas, tool routing and protocol result conversion.

use super::import::{start_import, ImportArg};
use super::resources::read_preview;
use super::{audit, ensure_write, validate_uid, MAX_PAGE_SIZE};
use crate::db::model::{Asset, AssetFilter};
use crate::error::{AppError, AppResult};
use crate::{media, services, AppState};
use base64::{engine::general_purpose::STANDARD, Engine};
use rmcp::handler::server::{router::tool::ToolRouter, wrapper::Parameters};
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::{tool, tool_handler, tool_router, ServerHandler};
use serde::Deserialize;
use serde_json::{json, Value};
use std::path::Path;
use tauri::{AppHandle, Manager};

#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct LibraryArg {
    library_id: Option<String>,
}

#[derive(Debug, Deserialize, Default, rmcp::schemars::JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct SearchFilters {
    /// Library-local numeric asset ID. Useful for restricting semantic search to
    /// one whole video.
    asset_id: Option<i64>,
    folder_id: Option<i64>,
    folder_ids: Vec<i64>,
    uncategorized: bool,
    tags: Vec<String>,
    kinds: Vec<String>,
    colors: Vec<String>,
    min_width: Option<i64>,
    min_height: Option<i64>,
    min_aspect: Option<f64>,
    max_aspect: Option<f64>,
    min_size: Option<i64>,
    max_size: Option<i64>,
    date_from: Option<i64>,
    date_to: Option<i64>,
    sort: Option<String>,
    visual_indexed: Option<bool>,
}

impl SearchFilters {
    pub(super) fn into_filter(
        self,
        filename: Option<String>,
        page: u32,
        page_size: u32,
    ) -> AssetFilter {
        AssetFilter {
            visual_indexed: self.visual_indexed,
            asset_id: self.asset_id,
            query: filename,
            folder_id: self.folder_id,
            folder_ids: self.folder_ids,
            uncategorized: self.uncategorized,
            tags: self.tags,
            kinds: self.kinds,
            colors: self.colors,
            min_width: self.min_width,
            min_height: self.min_height,
            min_aspect: self.min_aspect,
            max_aspect: self.max_aspect,
            min_size: self.min_size,
            max_size: self.max_size,
            date_from: self.date_from,
            date_to: self.date_to,
            page,
            page_size,
            sort: self.sort,
            ..Default::default()
        }
    }
}

#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct SearchArgs {
    library_id: Option<String>,
    semantic_query: Option<String>,
    filename_contains: Option<String>,
    #[serde(default)]
    filters: SearchFilters,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct AssetArg {
    pub(super) library_id: Option<String>,
    pub(super) asset_uid: String,
}

#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct CreateFolderArg {
    library_id: Option<String>,
    name: String,
}

#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct TagsArg {
    library_id: Option<String>,
    asset_uid: String,
    #[serde(default)]
    add: Vec<String>,
    #[serde(default)]
    remove: Vec<String>,
}

#[derive(Debug, Deserialize, rmcp::schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
struct MoveArg {
    library_id: Option<String>,
    asset_uids: Vec<String>,
    folder_id: Option<i64>,
}

pub(super) struct ClipSeekMcp<R: tauri::Runtime> {
    pub(super) app: AppHandle<R>,
    tool_router: ToolRouter<Self>,
}

impl<R: tauri::Runtime> Clone for ClipSeekMcp<R> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            tool_router: self.tool_router.clone(),
        }
    }
}

impl<R: tauri::Runtime> ClipSeekMcp<R> {
    pub(super) fn new(app: AppHandle<R>) -> Self {
        Self {
            app,
            tool_router: Self::tool_router(),
        }
    }

    pub(super) async fn blocking(
        &self,
        work: impl FnOnce(&AppState) -> AppResult<Value> + Send + 'static,
    ) -> CallToolResult {
        let app = self.app.clone();
        let result =
            tauri::async_runtime::spawn_blocking(move || work(&app.state::<AppState>())).await;
        match result {
            Ok(result) => tool_result(result),
            Err(error) => tool_result(Err(AppError::Msg(format!(
                "background task failed: {error}"
            )))),
        }
    }
}

#[tool_router(router = tool_router)]
impl<R: tauri::Runtime> ClipSeekMcp<R> {
    #[tool(description = "List ClipSeek libraries and identify the active desktop library")]
    async fn list_libraries(&self) -> CallToolResult {
        self.blocking(|state| {
            let config = state.config.read().unwrap();
            Ok(
                json!({"libraries": config.libraries.iter().map(|library| json!({
                "libraryId": library.id, "name": library.name,
                "isCurrent": config.active_library_id.as_deref() == Some(&library.id)
            })).collect::<Vec<_>>() }),
            )
        })
        .await
    }

    #[tool(description = "Get one library plus its folders and tags")]
    async fn get_library_context(
        &self,
        Parameters(args): Parameters<LibraryArg>,
    ) -> CallToolResult {
        self.blocking(move |state| {
            let session = services::session(state, args.library_id.as_deref())?;
            let (folders, tags) = services::folders_and_tags(&session)?;
            Ok(json!({
                "library": {"libraryId": session.meta.id, "name": session.meta.name},
                "folders": folders.iter().map(|folder| json!({"folderId": folder.id, "name": folder.name, "assetCount": folder.asset_count, "createdAt": folder.created_at})).collect::<Vec<_>>(),
                "tags": tags.iter().map(|tag| json!({"tagId": tag.id, "name": tag.name, "assetCount": tag.asset_count, "createdAt": tag.created_at})).collect::<Vec<_>>()
            }))
        }).await
    }

    #[tool(description = "Search assets by semantic text and/or metadata filters")]
    async fn search_assets(&self, Parameters(args): Parameters<SearchArgs>) -> CallToolResult {
        self.blocking(move |state| {
            let session = services::session(state, args.library_id.as_deref())?;
            let page = args.page.unwrap_or(1).max(1);
            let page_size = args.page_size.unwrap_or(20).clamp(1, MAX_PAGE_SIZE);
            let filter = args.filters.into_filter(args.filename_contains, page, page_size);
            if let Some(query) = args.semantic_query.as_deref().filter(|query| !query.trim().is_empty()) {
                let result = services::semantic_search(state, &session, query, &filter, page, page_size)?;
                let items = result.items.iter().map(|hit| asset_summary(&session.meta.id, &hit.asset, Some(hit))).collect::<Vec<_>>();
                Ok(json!({"items": items, "total": result.total, "page": result.page, "pageSize": result.page_size, "hasMore": result.has_more}))
            } else {
                let result = services::list_assets(&session, &filter)?;
                let total = if page == 1 {
                    result.total
                } else {
                    let mut count_filter = filter.clone();
                    count_filter.page = 1;
                    count_filter.page_size = 1;
                    services::list_assets(&session, &count_filter)?.total
                };
                let items = result.items.iter().map(|asset| asset_summary(&session.meta.id, asset, None)).collect::<Vec<_>>();
                Ok(json!({"items": items, "total": total, "page": result.page, "pageSize": result.page_size, "hasMore": result.has_more}))
            }
        }).await
    }

    #[tool(description = "Get a library asset record and preview resources by UID")]
    async fn get_asset(&self, Parameters(args): Parameters<AssetArg>) -> CallToolResult {
        self.blocking(move |state| {
            validate_uid(&args.asset_uid)?;
            let session = services::session(state, args.library_id.as_deref())?;
            let detail = services::asset_detail(&session, &args.asset_uid)?;
            Ok(asset_detail_value(&session.meta.id, &detail))
        })
        .await
    }

    #[tool(description = "Create a logical folder. Requires write access.")]
    async fn create_folder(&self, Parameters(args): Parameters<CreateFolderArg>) -> CallToolResult {
        self.blocking(move |state| {
            ensure_write(state)?;
            let session = services::session(state, args.library_id.as_deref())?;
            let folder = services::create_folder(&session, &args.name)?;
            audit(state, "create_folder", &session.meta.id, &[], true, None);
            Ok(json!({"folder": {"folderId": folder.id, "name": folder.name, "assetCount": folder.asset_count, "createdAt": folder.created_at}}))
        })
        .await
    }

    #[tool(description = "Atomically add and remove tags on an asset. Requires write access.")]
    async fn update_asset_tags(&self, Parameters(args): Parameters<TagsArg>) -> CallToolResult {
        self.blocking(move |state| {
            ensure_write(state)?;
            validate_uid(&args.asset_uid)?;
            let session = services::session(state, args.library_id.as_deref())?;
            let tags =
                services::update_asset_tags(&session, &args.asset_uid, &args.add, &args.remove)?;
            audit(
                state,
                "update_asset_tags",
                &session.meta.id,
                std::slice::from_ref(&args.asset_uid),
                true,
                None,
            );
            Ok(json!({"assetUid": args.asset_uid, "tags": tags}))
        })
        .await
    }

    #[tool(
        description = "Move up to 100 assets into a logical folder, or null for uncategorized. Requires write access."
    )]
    async fn move_assets_to_folder(&self, Parameters(args): Parameters<MoveArg>) -> CallToolResult {
        self.blocking(move |state| {
            ensure_write(state)?;
            for uid in &args.asset_uids {
                validate_uid(uid)?;
            }
            let session = services::session(state, args.library_id.as_deref())?;
            let moved =
                services::move_assets_to_folder(&session, &args.asset_uids, args.folder_id)?;
            audit(
                state,
                "move_assets_to_folder",
                &session.meta.id,
                &args.asset_uids,
                true,
                None,
            );
            Ok(json!({"moved": moved}))
        })
        .await
    }

    #[tool(
        description = "Import local media paths into a library. Returns an operation ID. Requires write access."
    )]
    async fn import_assets(&self, Parameters(args): Parameters<ImportArg>) -> CallToolResult {
        let app = self.app.clone();
        self.blocking(move |state| start_import(app, state, args))
            .await
    }

    #[tool(
        description = "Read a registered asset's file timestamps, media streams, embedded tags and photo EXIF by UID.",
        annotations(read_only_hint = true, open_world_hint = false)
    )]
    async fn get_asset_metadata(&self, Parameters(args): Parameters<AssetArg>) -> CallToolResult {
        self.blocking(move |state| {
            let session = services::session(state, args.library_id.as_deref())?;
            let _work = session.asset_activity.work()?;
            validate_uid(&args.asset_uid)?;
            let asset = services::asset_detail(&session, &args.asset_uid)?.asset;
            let metadata = media::metadata::read(Path::new(&asset.abs_path))?;
            Ok(json!({"libraryId": session.meta.id, "assetUid": asset.uid, "metadata": metadata}))
        })
        .await
    }

    #[tool(
        description = "Read the durable import operation and indexing progress in its original library"
    )]
    async fn get_operation(
        &self,
        Parameters(args): Parameters<super::import::OperationArg>,
    ) -> CallToolResult {
        self.blocking(move |state| super::import::operation_status(state, args))
            .await
    }
}

#[tool_handler(router = self.tool_router)]
impl<R: tauri::Runtime> ServerHandler for ClipSeekMcp<R> {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().enable_resources().build())
            .with_server_info(Implementation::new("clipseek-open", env!("CARGO_PKG_VERSION")))
            .with_instructions("Search and organize the user's local ClipSeek Open media library. Destructive operations are not available.")
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, ErrorData> {
        Ok(ListResourceTemplatesResult::with_all_items(vec![
            ResourceTemplate::new(
                "clipstudio-open://libraries/{libraryId}/assets/{assetUid}/preview",
                "asset-preview",
            )
            .with_description("A registered asset thumbnail or preview image"),
        ]))
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResponse, ErrorData> {
        let app = self.app.clone();
        let uri = request.uri.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            read_preview(&app.state::<AppState>(), &uri)
        })
        .await;
        match result {
            Ok(Ok((bytes, mime))) => Ok(ReadResourceResult::new(vec![ResourceContents::blob(
                STANDARD.encode(bytes),
                request.uri,
            )
            .with_mime_type(mime)])
            .into()),
            Ok(Err(error)) => Err(ErrorData::resource_not_found(
                error.to_string(),
                Some(json!({"code": error.code()})),
            )),
            Err(error) => Err(ErrorData::internal_error(error.to_string(), None)),
        }
    }
}

fn tool_result(result: AppResult<Value>) -> CallToolResult {
    match result {
        Ok(value) => CallToolResult::structured(value),
        Err(error) => CallToolResult::structured_error(json!(error)),
    }
}

fn asset_summary(
    library_id: &str,
    asset: &Asset,
    hit: Option<&crate::services::search::SearchHit>,
) -> Value {
    json!({
        "assetId": asset.id, "assetUid": asset.uid, "fileName": asset.file_name,
        "displayName": asset.display_name, "kind": asset.kind,
        "mimeType": asset.mime_type, "fileSize": asset.file_size, "width": asset.width,
        "height": asset.height, "durationMs": asset.duration_ms, "importedAt": asset.imported_at,
        "folderId": asset.folder_id, "folderName": asset.folder_name, "tags": asset.tags,
        "score": hit.map(|hit| hit.score),
        "previewUri": format!("clipstudio-open://libraries/{library_id}/assets/{}/preview", asset.uid)
    })
}

fn asset_detail_value(library_id: &str, detail: &crate::db::model::AssetDetail) -> Value {
    json!({"asset": asset_summary(library_id, &detail.asset, None),
        "color": detail.asset.color, "importStage": detail.asset.import_stage,
        "visualIndexed": detail.asset.visual_indexed})
}
