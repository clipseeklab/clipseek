pub(crate) mod activity;
mod dirs;
mod migration;
pub(crate) use migration::migrate_library_with_progress_and_close;
#[cfg(test)]
use migration::{copy_dir_resumable, directory_size, validate_library_copy};
pub use migration::{migrate_library, migrate_library_with_progress};
mod session;
pub use dirs::LibraryDirs;
pub(crate) use session::open_session_with_close;
pub use session::{open_session, open_session_light, open_session_read_only};

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::config::{GlobalConfig, LibraryMeta};
use crate::error::{AppError, AppResult};
use crate::util;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum VectorHealth {
    Ready,
    RebuildPending { code: String, message: String },
    Rebuilding { rebuild_id: String },
}

/// An open library session owns metadata, SQLite connections, and its vector index.
pub struct LibrarySession {
    pub meta: LibraryMeta,
    pub root: PathBuf,
    /// Protect rusqlite Connection with a mutex because it is not Sync.
    pub db: Mutex<Connection>,
    pub(crate) asset_activity: std::sync::Arc<activity::AssetActivity>,
    pub index_worker_running: std::sync::atomic::AtomicBool,
    /// Dedicated read connections keep lists and search independent of import write
    /// transactions.
    read_dbs: [Mutex<Connection>; 2],
    next_read_db: AtomicUsize,
    /// Optional Qdrant EdgeShard; failure disables search without preventing browsing.
    pub vector: Mutex<Option<qdrant_edge::EdgeShard>>,
    /// Serializable index health, separate from the live handle, supports diagnostics and
    /// recovery UI.
    pub vector_health: Mutex<VectorHealth>,
    /// Cancellation flag for user-requested index rebuilds.
    pub vector_rebuild_cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl LibrarySession {
    pub fn read_db(&self) -> std::sync::MutexGuard<'_, Connection> {
        let index = self.next_read_db.fetch_add(1, Ordering::Relaxed) % self.read_dbs.len();
        self.read_dbs[index].lock().unwrap()
    }
}

pub const LIBRARY_CONFIG_FILE: &str = "open-library.json";

/// Install the library's Finder icon on a best-effort basis; icon failures must not block
/// creation or opening.
#[cfg(target_os = "macos")]
pub fn install_library_folder_icon(root: &Path) -> AppResult<()> {
    const CUSTOM_ICON_FLAG: u16 = 0x0400;
    const INVISIBLE_FLAG: u16 = 0x4000;
    const ICON_VERSION: &str = "3";
    const ICON_BYTES: &[u8] = include_bytes!("../icons/library-folder.icns");

    let icon_path = root.join("Icon\r");
    let version_path = root.join(".clipstudio-open/folder-icon-version");
    let finder_info = std::process::Command::new("/usr/bin/xattr")
        .args(["-px", "com.apple.FinderInfo"])
        .arg(root)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .and_then(|raw| {
            let hex: String = raw.chars().filter(|c| c.is_ascii_hexdigit()).collect();
            if hex.len() != 64 {
                return None;
            }
            let mut bytes = [0u8; 32];
            for (index, byte) in bytes.iter_mut().enumerate() {
                *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
            }
            Some(bytes)
        })
        .unwrap_or([0; 32]);
    let flags = u16::from_be_bytes([finder_info[8], finder_info[9]]);
    if icon_path.is_file()
        && flags & CUSTOM_ICON_FLAG != 0
        && fs::read_to_string(&version_path).ok().as_deref() == Some(ICON_VERSION)
    {
        return Ok(());
    }

    let temp = tempfile::Builder::new()
        .prefix("clipseek-library-icon-")
        .tempdir()?;
    let source = temp.path().join("library.icns");
    fs::write(&source, ICON_BYTES)?;
    let sips = std::process::Command::new("/usr/bin/sips")
        .args(["-i"])
        .arg(&source)
        .output()?;
    if !sips.status.success() {
        return Err(AppError::coded(
            "library_icon_install_failed",
            "cannot prepare the ClipSeek folder icon",
        ));
    }
    // Clear the existing custom-icon flag before replacing resources so Finder does not
    // retain its inode-based cached icon.
    let mut cleared = finder_info;
    cleared[8..10].copy_from_slice(&(flags & !CUSTOM_ICON_FLAG).to_be_bytes());
    write_finder_info(root, &cleared)?;
    let _ = fs::remove_file(&icon_path);
    // Finder stores the custom icon in Icon\\r's resource fork; cp -p preserves extended
    // attributes and resource forks.
    let copy = std::process::Command::new("/bin/cp")
        .arg("-p")
        .arg(&source)
        .arg(&icon_path)
        .output()?;
    if !copy.status.success() {
        return Err(AppError::coded(
            "library_icon_install_failed",
            "cannot copy the ClipSeek folder icon",
        ));
    }

    // Hide Finder's Icon\\r implementation file from the library listing.
    let mut icon_finder_info = [0u8; 32];
    icon_finder_info[8..10].copy_from_slice(&INVISIBLE_FLAG.to_be_bytes());
    write_finder_info(&icon_path, &icon_finder_info)?;

    let mut updated = finder_info;
    let flags = flags | CUSTOM_ICON_FLAG;
    updated[8..10].copy_from_slice(&flags.to_be_bytes());
    write_finder_info(root, &updated)?;
    // Refresh directory timestamps so open Finder windows reload icon metadata.
    let _ = std::process::Command::new("/usr/bin/touch")
        .arg(root)
        .status();
    if let Some(parent) = version_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(version_path, ICON_VERSION)?;
    Ok(())
}

#[cfg(target_os = "macos")]
fn write_finder_info(path: &Path, finder_info: &[u8; 32]) -> AppResult<()> {
    let hex = finder_info
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let output = std::process::Command::new("/usr/bin/xattr")
        .args(["-wx", "com.apple.FinderInfo", &hex])
        .arg(path)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(AppError::coded(
            "library_icon_install_failed",
            "cannot update Finder icon metadata",
        ))
    }
}

#[cfg(target_os = "windows")]
pub fn install_library_folder_icon(root: &Path) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;

    const ICON_BYTES: &[u8] = include_bytes!("../icons/library-folder.ico");
    const FILE_ATTRIBUTE_READONLY: u32 = 0x1;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    const INI: &str = "[.ShellClassInfo]\r\nIconFile=.clipstudio\\library-folder.ico\r\nIconIndex=0\r\nConfirmFileOp=0\r\nClipSeekFolderIcon=1\r\n";

    let icon_path = root.join(".clipstudio-open/library-folder.ico");
    let ini_path = root.join("desktop.ini");
    let ini_bytes = std::iter::once(0xff)
        .chain(std::iter::once(0xfe))
        .chain(INI.encode_utf16().flat_map(u16::to_le_bytes))
        .collect::<Vec<_>>();

    if ini_path.exists() {
        let existing = fs::read(&ini_path)?;
        let owned = existing.starts_with(&[0xff, 0xfe])
            && String::from_utf16(
                &existing[2..]
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                    .collect::<Vec<_>>(),
            )
            .is_ok_and(|text| text.contains("ClipSeekFolderIcon=1"));
        if !owned {
            // Preserve a user's existing Explorer folder customization.
            return Ok(());
        }
    }

    let attributes_ok = windows_file_attributes(root)? & FILE_ATTRIBUTE_READONLY != 0
        && (!ini_path.exists()
            || windows_file_attributes(&ini_path)?
                & (FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM)
                == FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
    if attributes_ok
        && fs::read(&icon_path).ok().as_deref() == Some(ICON_BYTES)
        && fs::read(&ini_path).ok().as_deref() == Some(ini_bytes.as_slice())
    {
        return Ok(());
    }

    fs::create_dir_all(icon_path.parent().expect("icon has parent"))?;
    fs::write(&icon_path, ICON_BYTES)?;
    fs::write(&ini_path, ini_bytes)?;
    set_windows_file_attributes(&icon_path, FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM)?;
    set_windows_file_attributes(&ini_path, FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM)?;
    set_windows_file_attributes(root, FILE_ATTRIBUTE_READONLY)?;

    // Tell open Explorer windows to refresh the folder's cached icon.
    let wide = root
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    unsafe { SHChangeNotify(0x2000, 0x0005, wide.as_ptr().cast(), std::ptr::null()) };
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_file_attributes(path: &Path) -> AppResult<u32> {
    use std::os::windows::ffi::OsStrExt;
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let attributes = unsafe { GetFileAttributesW(wide.as_ptr()) };
    if attributes == u32::MAX {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(attributes)
    }
}

#[cfg(target_os = "windows")]
fn set_windows_file_attributes(path: &Path, attributes: u32) -> AppResult<()> {
    use std::os::windows::ffi::OsStrExt;
    let current = windows_file_attributes(path)?;
    let wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    if unsafe { SetFileAttributesW(wide.as_ptr(), current | attributes) } == 0 {
        Err(std::io::Error::last_os_error().into())
    } else {
        Ok(())
    }
}

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
extern "system" {
    fn GetFileAttributesW(path: *const u16) -> u32;
    fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
}

#[cfg(target_os = "windows")]
#[link(name = "shell32")]
extern "system" {
    fn SHChangeNotify(
        event: i32,
        flags: u32,
        item1: *const std::ffi::c_void,
        item2: *const std::ffi::c_void,
    );
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn install_library_folder_icon(_root: &Path) -> AppResult<()> {
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryConfig {
    pub version: u32,
    pub id: String,
    pub name: String,
    pub created_at: i64,
    #[serde(default)]
    pub color: Option<String>,
}

pub fn check_compatible(root: &Path) -> AppResult<()> {
    crate::db::check_compatible(&LibraryDirs::new(root).db())?;
    let path = root.join(LIBRARY_CONFIG_FILE);
    if path.is_file() {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
        if value
            .get("version")
            .and_then(|v| v.as_u64())
            .is_some_and(|v| v > 1)
        {
            return Err(AppError::coded(
                "library_upgrade_required",
                "Library format is newer; upgrade ClipSeek",
            ));
        }
    }
    Ok(())
}

pub fn write_library_config(root: &Path, meta: &LibraryMeta) -> AppResult<()> {
    check_compatible(root)?;
    let config = LibraryConfig {
        version: 1,
        id: meta.id.clone(),
        name: meta.name.clone(),
        created_at: meta.created_at,
        color: meta.color.clone(),
    };
    let path = root.join(LIBRARY_CONFIG_FILE);
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_string_pretty(&config)?)?;
    fs::rename(tmp, path)?;
    Ok(())
}

/// Initialize a library in an empty or absent directory without mutating global registration,
/// allowing isolated tests.
pub fn init_library(root: &Path, name: &str) -> AppResult<LibraryMeta> {
    validate_new_library_path(root, name)?;
    if root.exists() {
        let empty = fs::read_dir(root)
            .map(|mut it| it.next().is_none())
            .unwrap_or(false);
        if !empty {
            return Err(AppError::coded(
                "library_dir_not_empty",
                format!(
                    "directory already exists and is not empty: {}",
                    root.display()
                ),
            ));
        }
    } else {
        fs::create_dir_all(root)?;
    }
    let dirs = LibraryDirs::new(root);
    for d in [dirs.app(), dirs.images(), dirs.trash(), dirs.vector()] {
        fs::create_dir_all(d)?;
    }
    // Open once to initialize and migrate the database.
    let conn = crate::db::open_library_db(&dirs.db())?;
    drop(conn);

    let meta = LibraryMeta {
        id: util::new_id(),
        name: name.trim().to_string(),
        path: root.to_string_lossy().into_owned(),
        created_at: util::now(),
        color: None,
    };
    write_library_config(root, &meta)?;
    Ok(meta)
}

fn validate_library_name(name: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::InvalidArgument("library name is empty".into()));
    }
    if name
        .chars()
        .any(|ch| ch.is_control() || "<>:\"/\\|?*".contains(ch))
    {
        return Err(AppError::coded(
            "library_name_characters",
            "library name contains unsupported characters",
        ));
    }
    if name.ends_with(['.', ' ']) {
        return Err(AppError::coded(
            "library_name_trailing",
            "library name cannot end with a space or period",
        ));
    }
    let stem = name
        .trim_start()
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let device = (stem.starts_with("COM") || stem.starts_with("LPT"))
        && matches!(
            stem.get(3..).unwrap_or(""),
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
        );
    if device
        || matches!(
            stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
        )
    {
        return Err(AppError::coded(
            "library_name_reserved",
            "library name is reserved by the operating system",
        ));
    }
    if name.trim().len() > 255 {
        return Err(AppError::coded(
            "library_name_too_long",
            "library name exceeds 255 UTF-8 bytes",
        ));
    }
    Ok(())
}

fn validate_new_library_path(root: &Path, name: &str) -> AppResult<()> {
    validate_library_name(name)?;
    #[cfg(target_os = "windows")]
    validate_library_name(
        root.file_name()
            .and_then(|part| part.to_str())
            .unwrap_or(""),
    )?;
    #[cfg(not(target_os = "windows"))]
    let _ = root;
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/library_paths.rs"]
mod path_tests;

/// Register an existing library by directory or metadata file.
pub fn import_library(config: &mut GlobalConfig, selected: &str) -> AppResult<LibraryMeta> {
    let selected = PathBuf::from(selected);
    let root = if selected.is_dir() {
        selected
    } else {
        if selected.file_name().and_then(|n| n.to_str()) != Some(LIBRARY_CONFIG_FILE) {
            return Err(AppError::coded(
                "library_selection_invalid",
                format!("please select a library directory or {LIBRARY_CONFIG_FILE}"),
            ));
        }
        selected
            .parent()
            .ok_or_else(|| {
                AppError::coded("library_selection_invalid", "invalid library config path")
            })?
            .to_path_buf()
    };
    if !root.join(".clipstudio-open/library.db").is_file() {
        return Err(AppError::coded(
            "library_dir_invalid",
            "not a valid library directory (missing .clipstudio-open/library.db)",
        ));
    }
    check_compatible(&root)?;
    let raw = fs::read_to_string(root.join(LIBRARY_CONFIG_FILE)).map_err(|_| {
        AppError::coded(
            "library_config_missing",
            format!("library is missing {LIBRARY_CONFIG_FILE}"),
        )
    })?;
    let file: LibraryConfig = serde_json::from_str(&raw)?;
    if file.version > 1 {
        return Err(AppError::coded(
            "library_upgrade_required",
            "Library format is newer; upgrade ClipSeek",
        ));
    }
    crate::db::check_compatible(&root.join(".clipstudio-open/library.db"))?;
    if file.version != 1 || file.id.trim().is_empty() || file.name.trim().is_empty() {
        return Err(AppError::coded(
            "library_config_invalid",
            "library config file is invalid or uses an unsupported version",
        ));
    }
    let canonical = fs::canonicalize(&root).unwrap_or(root);
    if config.libraries.iter().any(|l| {
        l.id == file.id
            || fs::canonicalize(&l.path)
                .map(|existing| existing == canonical)
                .unwrap_or(false)
    }) {
        return Err(AppError::coded(
            "library_already_registered",
            "this library is already registered",
        ));
    }
    let meta = LibraryMeta {
        id: file.id,
        name: file.name,
        path: crate::config::user_friendly_path(&canonical.to_string_lossy()),
        created_at: file.created_at,
        color: file.color,
    };
    // Validate only SQLite here; the command layer opens the vector index through its session
    // cache to avoid duplicate WAL handles.
    let dirs = LibraryDirs::new(&canonical);
    drop(crate::db::open_library_db(&dirs.db())?);
    config.libraries.push(meta.clone());
    config.active_library_id = Some(meta.id.clone());
    config.save()?;
    Ok(meta)
}

/// Create a library and add its global registration.
pub fn create_library(
    config: &mut GlobalConfig,
    name: &str,
    path: &str,
    color: Option<&str>,
) -> AppResult<LibraryMeta> {
    let mut meta = init_library(&PathBuf::from(path), name)?;
    meta.color = color.map(str::to_string);
    write_library_config(Path::new(path), &meta)?;
    config.libraries.push(meta.clone());
    config.active_library_id = Some(meta.id.clone());
    config.save()?;
    Ok(meta)
}

/// Remove a library registration and optionally its managed directory.
pub fn delete_library(config: &mut GlobalConfig, id: &str, delete_files: bool) -> AppResult<()> {
    let Some(idx) = config.libraries.iter().position(|l| l.id == id) else {
        return Err(AppError::LibraryNotFound);
    };
    let meta = config.libraries[idx].clone();
    if delete_files {
        let root = PathBuf::from(&meta.path);
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
    }
    let mut updated = config.clone();
    updated.libraries.remove(idx);
    if updated.active_library_id.as_deref() == Some(id) {
        updated.active_library_id = updated.libraries.first().map(|l| l.id.clone());
    }
    updated.save()?;
    *config = updated;
    Ok(())
}

/// Relocate with same-volume rename or verified cross-volume copying.
pub fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &to)?;
        } else {
            fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/unit/library.rs"]
mod tests;
