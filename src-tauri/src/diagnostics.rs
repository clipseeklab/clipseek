use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
#[cfg(not(target_os = "windows"))]
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use zip::write::SimpleFileOptions;

use crate::{
    error::{AppError, AppResult},
    AppState,
};

const EXPORT_LIMIT: u64 = 20 * 1024 * 1024;

pub(crate) fn diagnostics_open_log_dir_blocking() -> AppResult<()> {
    let dir = crate::logging::log_dir();
    fs::create_dir_all(&dir)?;

    #[cfg(target_os = "macos")]
    let mut command = Command::new("open");
    #[cfg(target_os = "windows")]
    {
        open_log_directory(&dir)?;
    }
    #[cfg(target_os = "linux")]
    let mut command = Command::new("xdg-open");

    #[cfg(not(target_os = "windows"))]
    let status = command.arg(&dir).status()?;
    #[cfg(not(target_os = "windows"))]
    if !status.success() {
        return Err(AppError::Msg(format!(
            "failed to open log directory: {status}"
        )));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_log_directory(dir: &Path) -> AppResult<()> {
    tauri_plugin_opener::open_path(dir, None::<&str>)
        .map_err(|error| AppError::Msg(format!("failed to open log directory: {error}")))
}

#[cfg(all(test, target_os = "windows"))]
#[path = "../tests/unit/diagnostics.rs"]
mod tests;

pub(crate) fn diagnostics_export_blocking(
    state: &AppState,
    target_path: String,
) -> AppResult<String> {
    let target = PathBuf::from(target_path);
    let file = fs::File::create(&target)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let cfg = state.config.read().unwrap();
    let current = cfg
        .active_library_id
        .as_deref()
        .and_then(|id| cfg.library_by_id(id));
    // The library volume type helps diagnose network-volume lock failures and SQLite lock
    // storms.
    let volume_type = current
        .map(|x| {
            crate::volume::volume_type_name(std::path::Path::new(&x.path))
                .unwrap_or_else(|| "unknown".into())
        })
        .unwrap_or_else(|| "none".into());
    let report = format!(
        "created_at_ms={}\napp_version={}\nos={}\narch={}\nsession_id={}\nlibrary_count={}\ncurrent_library={}\nlibrary_volume_type={volume_type}\n",
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis(), env!("CARGO_PKG_VERSION"),
        std::env::consts::OS, std::env::consts::ARCH, crate::logging::session_id(), cfg.libraries.len(),
        current.map(|x| x.id.chars().take(8).collect::<String>()).unwrap_or_else(|| "none".into())
    );
    zip.start_file("report.txt", options).map_err(zip_error)?;
    zip.write_all(report.as_bytes())?;
    let mut remaining = EXPORT_LIMIT.saturating_sub(report.len() as u64);
    add_directory(
        &mut zip,
        &crate::logging::log_dir(),
        "logs",
        options,
        &mut remaining,
    )?;
    if let Ok(dir) = cfg.operations_dir() {
        add_operations(&mut zip, &dir, "operations/global", options, &mut remaining)?;
    }
    for library in &cfg.libraries {
        let dir = Path::new(&library.path).join(".clipstudio-open/operations");
        add_operations(
            &mut zip,
            &dir,
            &format!(
                "operations/{}",
                library.id.chars().take(8).collect::<String>()
            ),
            options,
            &mut remaining,
        )?;
    }
    drop(cfg);
    zip.finish().map_err(zip_error)?;
    crate::logging::info(
        "diagnostics",
        "exported",
        "-",
        &format!("bytes={}", fs::metadata(&target)?.len()),
    );
    Ok(target.to_string_lossy().into_owned())
}

fn add_operations(
    zip: &mut zip::ZipWriter<fs::File>,
    dir: &Path,
    prefix: &str,
    options: SimpleFileOptions,
    remaining: &mut u64,
) -> AppResult<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten().take(10) {
        let Ok(raw) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(mut progress) = serde_json::from_str::<crate::transfer::TransferProgress>(&raw)
        else {
            continue;
        };
        progress.current_item = None;
        progress.message = progress.message.map(|value| crate::logging::redact(&value));
        let data = serde_json::to_vec(&progress)?;
        if data.len() as u64 > *remaining {
            continue;
        }
        let name = entry
            .file_name()
            .to_string_lossy()
            .replace(['/', '\\'], "_");
        zip.start_file(format!("{prefix}/{name}"), options)
            .map_err(zip_error)?;
        zip.write_all(&data)?;
        *remaining = remaining.saturating_sub(data.len() as u64);
    }
    Ok(())
}

fn add_directory(
    zip: &mut zip::ZipWriter<fs::File>,
    dir: &Path,
    prefix: &str,
    options: SimpleFileOptions,
    remaining: &mut u64,
) -> AppResult<()> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Ok(());
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    entries.reverse();
    for entry in entries {
        let meta = entry.metadata()?;
        if !meta.is_file()
            || entry.path().extension().and_then(|x| x.to_str()) != Some("log")
            || meta.len() > *remaining
        {
            continue;
        }
        let mut data = Vec::with_capacity(meta.len() as usize);
        fs::File::open(entry.path())?
            .take(*remaining)
            .read_to_end(&mut data)?;
        let safe_name = entry
            .file_name()
            .to_string_lossy()
            .replace(['/', '\\'], "_");
        zip.start_file(format!("{prefix}/{safe_name}"), options)
            .map_err(zip_error)?;
        zip.write_all(&data)?;
        *remaining = remaining.saturating_sub(data.len() as u64);
    }
    Ok(())
}

fn zip_error(error: zip::result::ZipError) -> AppError {
    AppError::coded(
        "diagnostics_export_failed",
        format!("failed to create diagnostics bundle: {error}"),
    )
}
