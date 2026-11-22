//! Backend entry point: module registration, shared application state, and lifecycle.

#[cfg(all(feature = "e2e", not(debug_assertions)))]
compile_error!("The e2e automation feature must never be enabled in release builds");

pub mod clip;
pub mod commands;
pub mod config;
pub mod db;
pub mod diagnostics;
pub mod embedding;
pub mod error;
pub mod import;
pub mod indexing;
pub mod library;
mod library_startup;
pub mod logging;
pub mod mcp;
pub mod media;
pub mod model_download;
pub mod playback_media;
mod preview;
pub mod services;
pub mod transfer;
pub mod util;
pub mod vector;
pub mod volume;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub mod windows_runtime;
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
pub mod windows_webview2;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock, Weak};

use config::GlobalConfig;
use library::LibrarySession;
use tauri::Manager;

/// Authorize dynamic library paths for asset:// because users can create libraries outside
/// static scopes.
pub fn allow_library_assets<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &std::path::Path,
) -> error::AppResult<()> {
    let scope = app.asset_protocol_scope();
    // Derived images live under images; databases and indexes use the hidden application
    // directory.
    scope
        .allow_directory(root, true)
        .and_then(|_| scope.allow_directory(root.join(".clipstudio-open"), true))
        .map_err(|e| {
            error::AppError::Msg(format!(
                "failed to grant asset preview directory access: {e}"
            ))
        })?;
    debug_assert!(scope.is_allowed(root.join(".clipstudio-open")));
    Ok(())
}

/// Authorize reference sources per file rather than entire directories to minimize WebView
/// access.
pub fn allow_source_files<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    paths: &[std::path::PathBuf],
) {
    let scope = app.asset_protocol_scope();
    for path in paths {
        if let Err(e) = scope.allow_file(path) {
            logging::warn(
                "library",
                "source_scope_failed",
                "-",
                &format!("message={e}"),
            );
        }
    }
}

/// Restore all asset:// permissions on every session open because dynamic scopes do not
/// survive restarts.
pub fn allow_session_assets<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    session: &library::LibrarySession,
) -> error::AppResult<()> {
    allow_library_assets(app, &session.root)?;
    let sources = {
        let conn = session.db.lock().unwrap();
        db::assets::reference_source_paths(&conn)?
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>()
    };
    allow_source_files(app, &sources);
    Ok(())
}

/// Shared application state contains configuration, current session, import controls, and the
/// lazy encoder handle.
pub struct AppState {
    pub mcp: mcp::McpRuntime,
    pub config: RwLock<GlobalConfig>,
    pub session: RwLock<Option<Arc<LibrarySession>>>,
    /// Weak session cache prevents duplicate opens of the same Qdrant WAL.
    pub session_cache: Mutex<HashMap<String, Weak<LibrarySession>>>,
    /// Serialize creation, registration, and switching to avoid opening duplicate sessions
    /// for one path.
    pub session_switch_lock: Mutex<()>,
    pub import_ctl: import::ImportControl,
    pub clip: clip::ClipHandle,
    /// Serialize merges, transfers, and relocation so two writers cannot mutate the same
    /// library.
    pub operation_running: std::sync::atomic::AtomicBool,
    /// Reference-source checks share running and cancellation flags.
    pub source_check_running: Arc<std::sync::atomic::AtomicBool>,
    pub source_check_cancel: Arc<std::sync::atomic::AtomicBool>,
}

/// Apply the latest trash-retention preference daily; session recovery also performs
/// immediate startup cleanup.
fn spawn_daily_trash_cleanup(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(24 * 60 * 60));
        let state = app.state::<AppState>();
        let retention = state.config.read().unwrap().settings.trash_retention_days;
        let session = state.session.read().unwrap().clone();
        if let Some(session) = session {
            match commands::assets::auto_purge_expired(&session, retention) {
                Ok(count) if count > 0 => logging::info(
                    "trash",
                    "daily_auto_purge",
                    "-",
                    &format!("retention_days={retention};deleted={count}"),
                ),
                Ok(_) => {}
                Err(error) => logging::error(
                    "trash",
                    "daily_auto_purge_failed",
                    "-",
                    &format!("retention_days={retention};message={error}"),
                ),
            }
        }
    });
}

/// Restore the hidden main window for single-instance launches and macOS Dock activation.
fn reveal_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(feature = "e2e")]
    {
        let identity = serde_json::json!({"pid": std::process::id(), "boot": uuid::Uuid::new_v4().to_string()});
        std::fs::write(
            config::e2e_root().join("native-process.json"),
            identity.to_string(),
        )
        .expect("E2E process identity must be recorded in its sandbox");
    }
    logging::init();
    // Load global configuration and restore the last active library.
    let cfg = GlobalConfig::load_for_startup();
    let active_id = cfg.active_library_id.clone();
    let state = AppState {
        mcp: mcp::McpRuntime::default(),
        config: RwLock::new(cfg),
        session: RwLock::new(None),
        session_cache: Mutex::new(HashMap::new()),
        session_switch_lock: Mutex::new(()),
        import_ctl: import::ImportControl::default(),
        clip: clip::ClipHandle::default(),
        operation_running: std::sync::atomic::AtomicBool::new(false),
        source_check_running: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        source_check_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };

    let builder = tauri::Builder::default()
        .register_asynchronous_uri_scheme_protocol("asset", |context, request, responder| {
            let scope = context.app_handle().asset_protocol_scope();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(preview::response(request, |path| scope.is_allowed(path)));
            });
        })
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let _ = args;
            reveal_main_window(app);
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init());

    #[cfg(feature = "e2e")]
    let builder = builder
        .plugin(tauri_plugin_wdio::init())
        .plugin(tauri_plugin_wdio_webdriver::init());

    let app = builder
        .manage(state)
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                if let Err(error) = window.hide() {
                    logging::error(
                        "app",
                        "main_window_hide_failed",
                        "-",
                        &format!("message={error}"),
                    );
                } else {
                    logging::info(
                        "app",
                        "main_window_hidden",
                        "-",
                        "background_processing_continues=true",
                    );
                }
            }
        })
        .setup(move |app| {
            #[cfg(target_os = "windows")]
            {
                use tauri::menu::MenuBuilder;
                use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

                let menu = MenuBuilder::new(app)
                    .text("show", "Show main window")
                    .separator()
                    .text("quit", "Quit ClipSeek Open")
                    .build()?;
                TrayIconBuilder::with_id("main")
                    .icon(app.default_window_icon().expect("app icon").clone())
                    .tooltip("ClipSeek Open")
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        if event.id() == "show" {
                            reveal_main_window(app);
                        } else if event.id() == "quit" {
                            app.exit(0);
                        }
                    })
                    .on_tray_icon_event(|tray, event| {
                        if matches!(
                            event,
                            TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } | TrayIconEvent::DoubleClick {
                                button: MouseButton::Left,
                                ..
                            }
                        ) {
                            reveal_main_window(tray.app_handle());
                        }
                    })
                    .build(app)?;
            }
            model_download::configure(app.handle())?;
            app.state::<AppState>().clip.start_idle_cleanup();
            spawn_daily_trash_cleanup(app.handle().clone());
            let mcp_app = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = mcp::apply_settings(&mcp_app).await {
                    logging::error("mcp", "startup_failed", "-", &error.to_string());
                }
            });
            // Open the initial library off the event loop. The initial current-library
            // command waits for readiness on its worker, keeping the native window responsive.
            if let Some(id) = active_id {
                let completion = library_startup::begin();
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    let _completion = completion;
                    let state = handle.state::<AppState>();
                    let _switch_guard = state.session_switch_lock.lock().unwrap();
                    if state.config.read().unwrap().active_library_id.as_deref()
                        != Some(id.as_str())
                    {
                        return;
                    }
                    let cfg = state.config.read().unwrap().clone();
                    if let Some(meta) = cfg.library_by_id(&id).cloned() {
                        // Best-effort Finder icon installation must not prevent legacy
                        // session recovery.
                        commands::library::ensure_library_folder_icon(&meta);
                        match services::sessions::open_cached_session(&state, meta) {
                            Ok(s) => {
                                if let Err(e) = allow_session_assets(&handle, &s) {
                                    logging::error(
                                        "library",
                                        "asset_scope_failed",
                                        "-",
                                        &format!("message={e}"),
                                    );
                                }
                                commands::library::warn_if_network_volume(&handle, &s);
                                // Clean expired trash at startup.
                                let retention = cfg.settings.trash_retention_days;
                                let purge_session = s.clone();
                                std::thread::spawn(move || {
                                    if let Err(e) = commands::assets::auto_purge_expired(
                                        &purge_session,
                                        retention,
                                    ) {
                                        logging::error(
                                            "trash",
                                            "auto_purge_failed",
                                            "-",
                                            &format!("message={e}"),
                                        );
                                    }
                                });
                                *state.session.write().unwrap() = Some(s.clone());

                                // Recover legacy derived inputs and interrupted embeddings.
                                services::maintenance::spawn_library_maintenance(&handle, s);
                            }
                            Err(e) => logging::error(
                                "library",
                                "restore_session_failed",
                                "-",
                                &format!("message={e}"),
                            ),
                        }
                    }
                });
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::hover_video_play,
            // Library management.
            commands::library::library_list,
            commands::library::library_current,
            commands::library::library_volume_warning,
            commands::library::library_operations,
            commands::library::library_create,
            commands::library::library_import,
            commands::library::library_switch,
            commands::library::library_reauthorize,
            commands::library::library_rename,
            commands::library::library_delete,
            commands::library::library_migrate,
            commands::library::library_merge,
            commands::library::asset_move_to_library,
            // Assets.
            commands::assets::asset_list,
            commands::assets::asset_storage_stats,
            commands::assets::source_check,
            commands::assets::source_check_cancel,
            commands::assets::asset_detail,
            commands::assets::asset_metadata,
            commands::assets::asset_set_display_name,
            commands::assets::asset_set_cover,
            commands::assets::asset_preview,
            commands::assets::asset_full_preview,
            commands::assets::frame_preview,
            commands::assets::asset_playback_source,
            commands::assets::asset_playback_codec,
            commands::assets::asset_export,
            commands::assets::asset_add_tags,
            commands::assets::asset_remove_tag,
            commands::assets::asset_move_to_folder,
            commands::assets::asset_trash,
            commands::assets::asset_restore,
            commands::assets::asset_delete_permanent,
            commands::assets::trash_purge_all,
            // Folders.
            commands::folders::folder_list,
            commands::folders::folder_page,
            commands::folders::folder_get,
            commands::folders::folder_sidebar,
            commands::folders::folder_sidebar_set,
            commands::folders::folder_pin,
            commands::folders::folder_create,
            commands::folders::folder_rename,
            commands::folders::folder_delete,
            commands::folders::folder_batch,
            // Tags.
            commands::tags::tag_list,
            commands::tags::tag_create_many,
            commands::tags::tag_batch,
            commands::tags::target_options,
            commands::tags::tag_rename,
            commands::tags::tag_delete,
            // Semantic search.
            commands::search::search_text,
            commands::search::search_image,
            commands::search::search_image_preview,
            commands::processing::processing_list,
            commands::processing::processing_summary,
            commands::processing::processing_history,
            commands::processing::processing_hide,
            commands::processing::processing_clear,
            commands::processing::processing_cancel,
            commands::processing::processing_retry,
            commands::indexing::index_summary,
            commands::indexing::index_build,
            commands::indexing::index_tasks,
            commands::indexing::index_task_cancel,
            commands::indexing::index_task_retry,
            commands::vector_recovery::vector_status,
            commands::vector_recovery::vector_retry_open,
            commands::vector_recovery::vector_rebuild_preflight,
            commands::vector_recovery::vector_rebuild_start,
            commands::vector_recovery::vector_rebuild_cancel,
            // Settings.
            commands::settings::mcp_status,
            commands::settings::mcp_token_reveal,
            commands::settings::mcp_token_rotate,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::tray_set_labels,
            commands::settings::onboarding_get,
            commands::settings::onboarding_set,
            commands::diagnostics::diagnostics_open_log_dir,
            commands::diagnostics::diagnostics_export,
            commands::diagnostics::frontend_error,
            commands::model::model_status,
            commands::model::model_settings,
            model_download::model_download,
            // Import.
            commands::import::import_paths,
            commands::import::import_folders,
            commands::import::import_cancel,
            commands::import::import_running,
            commands::import::import_retry,
            commands::import::import_recovery_list,
            commands::import::import_recovery_resume,
            commands::import::import_recovery_abandon,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application");
    app.run(|_app, _event| {
        if matches!(_event, tauri::RunEvent::Exit) {
            _app.state::<AppState>().mcp.stop();
        }
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen {
            has_visible_windows: false,
            ..
        } = _event
        {
            reveal_main_window(_app);
        }
    });
    logging::shutdown();
}
