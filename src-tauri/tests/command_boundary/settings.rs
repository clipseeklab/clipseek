use super::*;

#[test]
fn diagnostics_export_preserves_target_path_and_returns_the_io_error_contract() {
    let app = test_app();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("missing-parent/logs.zip");
    let error = invoke(
        &webview,
        "diagnostics_export",
        json!({ "targetPath": target.to_string_lossy() }),
    )
    .unwrap_err();
    assert_error_code(&error, "io");
    assert!(!target.exists());
}

#[test]
fn rejected_settings_and_failed_saves_do_not_publish_partial_state() {
    for invalid in [true, false] {
        let app = test_app();
        let temp = tempfile::tempdir().unwrap();
        let parent = temp.path().join("config-parent");
        *app.state::<AppState>().config.write().unwrap() =
            GlobalConfig::load_from(parent.join("config.json")).unwrap();
        if !invalid {
            std::fs::write(&parent, b"not a directory").unwrap();
        }
        let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
            .build()
            .unwrap();
        let body = if invalid {
            json!({"theme":"dark", "importMode":"invalid"})
        } else {
            json!({"theme":"dark"})
        };
        assert!(invoke(&webview, "settings_set", body).is_err());
        assert_eq!(
            app.state::<AppState>()
                .config
                .read()
                .unwrap()
                .settings
                .theme,
            "auto"
        );
        assert!(!parent.join("config.json").exists());
    }
}

#[test]
fn failed_onboarding_save_does_not_advance_the_in_memory_wizard() {
    let app = test_app();
    let temp = tempfile::tempdir().unwrap();
    let parent = temp.path().join("not-a-directory");
    *app.state::<AppState>().config.write().unwrap() =
        GlobalConfig::load_from(parent.join("config.json")).unwrap();
    std::fs::write(&parent, b"blocked").unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    assert!(invoke(
        &webview,
        "onboarding_set",
        json!({"currentStep":3,"completed":true})
    )
    .is_err());
    assert_eq!(
        app.state::<AppState>()
            .config
            .read()
            .unwrap()
            .onboarding
            .current_step,
        1
    );
}

#[test]
fn destructive_commands_use_the_library_that_sent_the_request() {
    let f = export_fixture();
    let source = f.session.meta.clone();
    let other =
        clip_studio_lib::library::init_library(&f.temp.path().join("other"), "Other").unwrap();
    {
        let state = f._app.state::<AppState>();
        state
            .config
            .write()
            .unwrap()
            .libraries
            .extend([source.clone(), other.clone()]);
        state
            .session_cache
            .lock()
            .unwrap()
            .insert(source.id.clone(), Arc::downgrade(&f.session));
        *state.session.write().unwrap() = Some(Arc::new(
            clip_studio_lib::library::open_session(other).unwrap(),
        ));
    }
    let mut headers = tauri::http::HeaderMap::new();
    headers.insert("x-clipseek-library", source.id.parse().unwrap());
    invoke_with_headers(
        &f.webview,
        "asset_trash",
        json!({"ids":[f.asset_id]}),
        headers,
    )
    .unwrap();
    assert!(clip_studio_lib::db::assets::get_asset(
        &f.session.read_db(),
        &f.session.root,
        f.asset_id
    )
    .unwrap()
    .unwrap()
    .deleted_at
    .is_some());
}

#[test]
fn mcp_settings_start_stop_and_tokens_cross_the_real_ipc_boundary() {
    let app = test_app();
    let temp = tempfile::tempdir().unwrap();
    *app.state::<AppState>().config.write().unwrap() =
        GlobalConfig::load_from(temp.path().join("config.json")).unwrap();
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .unwrap();
    let defaults = invoke(&webview, "settings_get", json!({})).unwrap();
    assert_eq!(defaults["mcpEnabled"], false);
    assert_eq!(defaults["mcpWriteEnabled"], false);
    assert_eq!(defaults["mcpListenMode"], "loopback");
    let enabled = invoke(&webview, "settings_set", json!({"mcpEnabled":true})).unwrap();
    assert_eq!(enabled["mcpEnabled"], true);
    assert_eq!(enabled["mcpWriteEnabled"], false);
    assert!(enabled["mcpPort"].as_u64().unwrap() > 0);
    let status = invoke(&webview, "mcp_status", json!({})).unwrap();
    assert_eq!(status["running"], true);
    assert!(status["url"]
        .as_str()
        .unwrap()
        .starts_with("http://127.0.0.1:"));
    let token = invoke(&webview, "mcp_token_reveal", json!({})).unwrap();
    let rotated = invoke(&webview, "mcp_token_rotate", json!({})).unwrap();
    assert_eq!(token.as_str().unwrap().len(), 64);
    assert_ne!(token, rotated);
    assert!(invoke(
        &webview,
        "settings_set",
        json!({"mcpEnabled":false,"mcpListenMode":"invalid"})
    )
    .is_err());
    assert_eq!(
        invoke(&webview, "mcp_status", json!({})).unwrap()["running"],
        true
    );
    assert_eq!(
        invoke(&webview, "settings_set", json!({"mcpWriteEnabled":true})).unwrap()
            ["mcpWriteEnabled"],
        true
    );
    let disabled = invoke(&webview, "settings_set", json!({"mcpEnabled":false})).unwrap();
    assert_eq!(disabled["mcpWriteEnabled"], false);
    assert_eq!(
        invoke(&webview, "mcp_status", json!({})).unwrap()["running"],
        false
    );
    let restarted = invoke(&webview, "settings_set", json!({"mcpEnabled":true})).unwrap();
    assert_eq!(restarted["mcpPort"], enabled["mcpPort"]);
    assert_eq!(
        invoke(&webview, "mcp_status", json!({})).unwrap()["running"],
        true
    );
    invoke(&webview, "settings_set", json!({"mcpEnabled":false})).unwrap();
    assert!(temp.path().join("mcp-token").exists());
}
