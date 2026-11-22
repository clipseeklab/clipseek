use crate::commands::with_session;
use crate::db::{self, model::Folder};
use crate::error::{AppError, AppResult};
use crate::AppState;

use crate::services::folders::validate_folder_name;
use crate::services::folders::{create_folder, delete_folder, rename_folder};

#[tauri::command]
pub async fn folder_page<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    query: String,
    page: i64,
    sort: Option<String>,
) -> AppResult<db::folders::FolderPage> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |s| {
            db::folders::page_sorted(
                &s.read_db(),
                &query,
                page,
                sort.as_deref().unwrap_or("created_desc"),
            )
        })
    })
    .await
}
#[tauri::command]
pub async fn folder_get<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
) -> AppResult<Option<Folder>> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |s| db::folders::get(&s.read_db(), id))
    })
    .await
}
#[tauri::command]
pub async fn folder_sidebar<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
) -> AppResult<db::folders::FolderSidebar> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |s| db::folders::sidebar(&s.db.lock().unwrap()))
    })
    .await
}
#[tauri::command]
pub async fn folder_sidebar_set<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
    ids: Vec<i64>,
    display_limit: Option<i64>,
) -> AppResult<db::folders::FolderSidebar> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |s| {
            if s.meta.id != library_id {
                return Err(AppError::LibraryNotOpen);
            }
            let mut conn = s.db.lock().unwrap();
            let tx = conn.transaction()?;
            let result = db::folders::set_sidebar_with_limit(&tx, &ids, display_limit)?;
            tx.commit()?;
            Ok(result)
        })
    })
    .await
}
#[tauri::command]
pub async fn folder_pin<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    library_id: String,
    id: i64,
    pinned: bool,
) -> AppResult<db::folders::FolderSidebar> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |s| {
            if s.meta.id != library_id {
                return Err(AppError::LibraryNotOpen);
            }
            db::folders::pin(&s.db.lock().unwrap(), id, pinned)
        })
    })
    .await
}

/// List folders.
#[tauri::command]
pub async fn folder_list(app: tauri::AppHandle) -> AppResult<Vec<Folder>> {
    crate::commands::blocking(app, folder_list_blocking).await
}

fn folder_list_blocking(state: &AppState) -> AppResult<Vec<Folder>> {
    with_session(state, |s| {
        let conn = s.read_db();
        db::folders::list(&conn)
    })
}

/// Create a logical folder.
#[tauri::command]
pub async fn folder_create<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    name: String,
) -> AppResult<Folder> {
    crate::commands::blocking(app, move |state| folder_create_blocking(state, name)).await
}

pub(crate) fn folder_create_blocking(state: &AppState, name: String) -> AppResult<Folder> {
    let name = validate_folder_name(&name)?;
    with_session(state, |s| create_folder(s, &name))
}

/// Rename a logical folder without changing physical asset paths.
#[tauri::command]
pub async fn folder_rename<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    id: i64,
    new_name: String,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        folder_rename_blocking(state, id, new_name)
    })
    .await
}

pub(crate) fn folder_rename_blocking(state: &AppState, id: i64, new_name: String) -> AppResult<()> {
    let new_name = validate_folder_name(&new_name)?;
    with_session(state, |s| rename_folder(s, id, &new_name))
}

/// Trash member assets before deleting the folder row.
#[tauri::command]
pub async fn folder_delete<R: tauri::Runtime>(app: tauri::AppHandle<R>, id: i64) -> AppResult<()> {
    crate::commands::blocking(app, move |state| folder_delete_blocking(state, id)).await
}

pub(crate) fn folder_delete_blocking(state: &AppState, id: i64) -> AppResult<()> {
    with_session(state, |s| delete_folder(s, id))
}

#[tauri::command]
pub async fn folder_batch<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    ids: Vec<i64>,
    target_id: Option<i64>,
) -> AppResult<()> {
    crate::commands::blocking(app, move |state| {
        with_session(state, |session| {
            crate::services::folders::batch(session, &ids, target_id)
        })
    })
    .await
}
