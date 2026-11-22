//! MCP imports share the desktop's durable, reference-only queue and library binding.
use super::{audit, ensure_write, validate_uid};
use crate::{
    db,
    error::{AppError, AppResult},
    services, AppState,
};
use rmcp::schemars::JsonSchema;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::AppHandle;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct ImportArg {
    library_id: Option<String>,
    paths: Vec<String>,
    folder_id: Option<i64>,
}
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(super) struct OperationArg {
    library_id: Option<String>,
    operation_id: String,
}

pub(super) fn start_import<R: tauri::Runtime>(
    app: AppHandle<R>,
    state: &AppState,
    args: ImportArg,
) -> AppResult<Value> {
    ensure_write(state)?;
    if args.paths.is_empty()
        || args.paths.len() > 100
        || args
            .paths
            .iter()
            .any(|path| !std::path::Path::new(path).is_absolute())
    {
        return Err(AppError::InvalidArgument(
            "Provide 1..100 absolute local paths".into(),
        ));
    }
    let session = services::session(state, args.library_id.as_deref())?;
    let operation_id = crate::util::new_entity_uid();
    let accepted = services::imports::submit_in_session(
        app,
        state,
        session.clone(),
        services::imports::PathImport {
            paths: args.paths,
            folder_id: args.folder_id,
            batch_id: Some(operation_id.clone()),
            mode: Some(crate::import::ImportMode::Reference),
            ..Default::default()
        },
    )?;
    audit(
        state,
        "import_assets",
        &session.meta.id,
        &[],
        true,
        Some(accepted.queued),
    );
    Ok(json!({"operationId":operation_id,"libraryId":session.meta.id,"queued":accepted.queued}))
}

pub(super) fn operation_status(state: &AppState, args: OperationArg) -> AppResult<Value> {
    validate_uid(&args.operation_id)?;
    let session = services::session(state, args.library_id.as_deref())?;
    let result = db::import_jobs::operation_status(&session.read_db(), &args.operation_id)?
        .ok_or_else(|| AppError::InvalidArgument("Unknown operationId in this library".into()))?;
    Ok(json!({"operationId":args.operation_id,"libraryId":session.meta.id,"summary":result}))
}
