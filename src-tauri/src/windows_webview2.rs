//! Detect the Evergreen WebView2 Runtime before Tauri creates a window.

use std::ffi::c_void;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ};
use winreg::RegKey;

const CLIENT_ID: &str = "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
const DOWNLOAD_URL: &str = "https://developer.microsoft.com/en-us/microsoft-edge/webview2/consumer";

pub fn valid_runtime_version(value: &str) -> bool {
    let components = value.split('.').collect::<Vec<_>>();
    components.len() == 4
        && components
            .iter()
            .all(|part| !part.is_empty() && part.parse::<u32>().is_ok())
        && components
            .iter()
            .any(|part| part.parse::<u32>().unwrap_or(0) > 0)
}

pub fn is_available() -> bool {
    let machine = format!("SOFTWARE\\WOW6432Node\\Microsoft\\EdgeUpdate\\Clients\\{CLIENT_ID}");
    let user = format!("Software\\Microsoft\\EdgeUpdate\\Clients\\{CLIENT_ID}");
    [(HKEY_LOCAL_MACHINE, machine), (HKEY_CURRENT_USER, user)]
        .into_iter()
        .any(|(hive, path)| {
            RegKey::predef(hive)
                .open_subkey_with_flags(path, KEY_READ)
                .ok()
                .and_then(|key| key.get_value::<String, _>("pv").ok())
                .is_some_and(|version| valid_runtime_version(&version))
        })
}

pub fn show_missing_dialog() {
    let title = wide("ClipSeek Open - WebView2 required");
    let message = wide("ClipSeek Open requires Microsoft Edge WebView2 Runtime. Click Yes to open Microsoft's download page, then restart ClipSeek Open.");
    // MB_YESNO | MB_ICONWARNING. Nothing is downloaded by the installer.
    let response =
        unsafe { MessageBoxW(std::ptr::null_mut(), message.as_ptr(), title.as_ptr(), 0x34) };
    if response == 6 {
        let _ = std::process::Command::new("explorer.exe")
            .arg(DOWNLOAD_URL)
            .spawn();
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(window: *mut c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
}
