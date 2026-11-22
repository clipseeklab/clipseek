//! Compact, bounded local diagnostics. Log storage is capped independently of callers.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const LOG_DIR_LIMIT: u64 = 20 * 1024 * 1024;
const REGULAR_LOG_LIMIT: u64 = 16 * 1024 * 1024;
const CRASH_LOG_LIMIT: u64 = 4 * 1024 * 1024;
const SEGMENT_LIMIT: u64 = 4 * 1024 * 1024;
const LINE_LIMIT: usize = 4 * 1024;
const RETENTION: Duration = Duration::from_secs(72 * 60 * 60);
const REPEAT_WINDOW: Duration = Duration::from_secs(10);

static LOGGER: OnceLock<mpsc::SyncSender<Record>> = OnceLock::new();
static SESSION_ID: OnceLock<String> = OnceLock::new();
static DISCONNECTED_REPORTED: AtomicBool = AtomicBool::new(false);
static DROPPED_INFO: AtomicU64 = AtomicU64::new(0);
static DROPPED_WARN: AtomicU64 = AtomicU64::new(0);
static DROPPED_ERROR: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
struct Record {
    level: char,
    module: String,
    event: String,
    operation: String,
    detail: String,
}

struct Repeat {
    first: SystemTime,
    count: u64,
    record: Record,
}

#[cfg(feature = "e2e")]
pub fn log_dir() -> PathBuf {
    crate::config::e2e_root().join("logs")
}

#[cfg(not(feature = "e2e"))]
pub fn log_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Logs/org.clipstudio.open");
    }
    #[cfg(target_os = "windows")]
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        return PathBuf::from(local).join("org.clipstudio.open/logs");
    }
    std::env::temp_dir().join("org.clipstudio.open/logs")
}

pub fn session_id() -> &'static str {
    SESSION_ID.get_or_init(|| crate::util::new_entity_uid().chars().take(8).collect())
}

pub fn init() {
    let (sender, receiver) = mpsc::sync_channel::<Record>(2_000);
    if LOGGER.set(sender).is_err() {
        return;
    }
    let _ = fs::create_dir_all(log_dir());
    cleanup(&log_dir());
    let marker = log_dir().join("running.marker");
    let previous_abnormal = marker.exists();
    let _ = fs::write(&marker, session_id());
    std::thread::Builder::new()
        .name("clip-log".into())
        .spawn(move || worker(receiver))
        .ok();
    install_panic_hook();
    info(
        "app",
        "started",
        "-",
        &format!(
            "ver={};arch={}",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH
        ),
    );
    if previous_abnormal {
        error("app", "previous_exit_abnormal", "-", "detected=1");
    }
}

pub fn shutdown() {
    info("app", "shutdown", "-", "reason=normal");
    std::thread::sleep(Duration::from_millis(30));
    let _ = fs::remove_file(log_dir().join("running.marker"));
}

pub fn info(module: &str, event: &str, operation: &str, detail: &str) {
    send('I', module, event, operation, detail);
}
pub fn warn(module: &str, event: &str, operation: &str, detail: &str) {
    send('W', module, event, operation, detail);
}
pub fn error(module: &str, event: &str, operation: &str, detail: &str) {
    send('E', module, event, operation, detail);
}

fn send(level: char, module: &str, event: &str, operation: &str, detail: &str) {
    let record = Record {
        level,
        module: field(module),
        event: field(event),
        operation: field(&redact(operation)),
        detail: redact(detail),
    };
    if let Some(sender) = LOGGER.get() {
        match sender.try_send(record) {
            Ok(()) => {}
            Err(mpsc::TrySendError::Full(record)) => {
                match record.level {
                    'E' => &DROPPED_ERROR,
                    'W' => &DROPPED_WARN,
                    _ => &DROPPED_INFO,
                }
                .fetch_add(1, Ordering::Relaxed);
            }
            Err(mpsc::TrySendError::Disconnected(record)) => {
                if !DISCONNECTED_REPORTED.swap(true, Ordering::Relaxed) {
                    let mut detail = record.detail;
                    truncate_utf8(&mut detail, LINE_LIMIT);
                    let _ = writeln!(std::io::stderr(), "ClipSeek logger disconnected; {detail}");
                }
            }
        }
    }
}

fn field(value: &str) -> String {
    value
        .replace(['|', '\n', '\r'], "_")
        .chars()
        .take(80)
        .collect()
}

/// Redact newlines, separators, user paths, emails, serial numbers, and URL query strings
/// from logs.
pub fn redact(value: &str) -> String {
    let mut out = value.replace(['\n', '\r', '|'], " ");
    let roots: Vec<String> = ["HOME", "USERPROFILE", "APPDATA", "LOCALAPPDATA"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(|value| value.to_string_lossy().into_owned())
        .collect();
    out = redact_paths(&out, &roots);
    // Avoid accidentally persisting URL credentials/query strings.
    out.split_whitespace()
        .map(|part| {
            if part.contains('@') {
                "<email>".to_string()
            } else if part.starts_with("CS-") {
                "<serial>".to_string()
            } else if part.starts_with("http://") || part.starts_with("https://") {
                part.split('?').next().unwrap_or(part).to_string()
            } else {
                part.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn redact_paths(value: &str, roots: &[String]) -> String {
    let mut variants: Vec<String> = roots
        .iter()
        .flat_map(|root| {
            [
                root.clone(),
                root.replace('\\', "/"),
                root.replace('/', "\\"),
            ]
        })
        .filter(|root| !root.is_empty())
        .collect();
    variants.sort_by_key(|root| std::cmp::Reverse(root.len()));
    variants.dedup();
    let mut out = value.to_string();
    for root in variants {
        let needle = root.to_ascii_lowercase();
        while let Some(start) = out.to_ascii_lowercase().find(&needle) {
            out.replace_range(start..start + root.len(), "<home>");
        }
    }
    out
}

#[cfg(test)]
#[path = "../tests/unit/logging_paths.rs"]
mod path_tests;

fn worker(receiver: mpsc::Receiver<Record>) {
    let dir = log_dir();
    let mut writer: Option<BufWriter<File>> = None;
    let mut repeats: HashMap<String, Repeat> = HashMap::new();
    let mut written = 0u64;
    loop {
        let record = match receiver.recv_timeout(Duration::from_secs(1)) {
            Ok(record) => record,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                flush_dropped(&mut writer, &dir, &mut written);
                flush_repeats(
                    &mut repeats,
                    SystemTime::now(),
                    &mut writer,
                    &dir,
                    &mut written,
                    false,
                );
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        };
        let now = SystemTime::now();
        flush_repeats(&mut repeats, now, &mut writer, &dir, &mut written, false);
        let key = format!(
            "{}|{}|{}|{}",
            record.level, record.module, record.event, record.operation
        );
        if let Some(repeat) = repeats.get_mut(&key) {
            if now.duration_since(repeat.first).unwrap_or_default() < REPEAT_WINDOW {
                repeat.count += 1;
                continue;
            }
        }
        if repeats.len() >= 256 {
            flush_repeats(&mut repeats, now, &mut writer, &dir, &mut written, true);
        }
        repeats.insert(
            key,
            Repeat {
                first: now,
                count: 1,
                record: record.clone(),
            },
        );
        write_record(&record, &mut writer, &dir, &mut written);
        flush_dropped(&mut writer, &dir, &mut written);
    }
}

fn flush_dropped(writer: &mut Option<BufWriter<File>>, dir: &Path, written: &mut u64) {
    let info = DROPPED_INFO.swap(0, Ordering::Relaxed);
    let warn = DROPPED_WARN.swap(0, Ordering::Relaxed);
    let error = DROPPED_ERROR.swap(0, Ordering::Relaxed);
    if info + warn + error == 0 {
        return;
    }
    write_record(
        &Record {
            level: 'W',
            module: "log".into(),
            event: "records_dropped".into(),
            operation: "-".into(),
            detail: format!("info={info};warn={warn};error={error}"),
        },
        writer,
        dir,
        written,
    );
}

fn flush_repeats(
    repeats: &mut HashMap<String, Repeat>,
    now: SystemTime,
    writer: &mut Option<BufWriter<File>>,
    dir: &Path,
    written: &mut u64,
    all: bool,
) {
    let keys: Vec<_> = repeats
        .iter()
        .filter(|(_, r)| all || now.duration_since(r.first).unwrap_or_default() >= REPEAT_WINDOW)
        .map(|(k, _)| k.clone())
        .collect();
    for key in keys {
        if let Some(repeat) = repeats.remove(&key) {
            if repeat.count > 1 {
                let summary = Record {
                    level: 'W',
                    module: "log".into(),
                    event: "repeated".into(),
                    operation: repeat.record.operation,
                    detail: format!(
                        "source={}.{};count={};window_ms=10000",
                        repeat.record.module,
                        repeat.record.event,
                        repeat.count - 1
                    ),
                };
                write_record(&summary, writer, dir, written);
            }
        }
    }
}

fn write_record(
    record: &Record,
    writer: &mut Option<BufWriter<File>>,
    dir: &Path,
    written: &mut u64,
) {
    if writer.is_none() || *written >= SEGMENT_LIMIT {
        if let Some(mut old) = writer.take() {
            let _ = old.flush();
        }
        cleanup(dir);
        let stamp = now_ms();
        let path = dir.join(format!("clip-studio-{stamp}.log"));
        *writer = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()
            .map(BufWriter::new);
        *written = 0;
    }
    let mut line = format!(
        "{}|{}|{}|{}|{}|{}|{}\n",
        now_ms(),
        record.level,
        session_id(),
        record.module,
        record.event,
        record.operation,
        record.detail
    );
    if line.len() > LINE_LIMIT {
        truncate_utf8(&mut line, LINE_LIMIT.saturating_sub(14));
        line.push_str(";truncated=1\n");
    }
    if let Some(output) = writer.as_mut() {
        if output.write_all(line.as_bytes()).is_ok() {
            let _ = output.flush();
            *written += line.len() as u64;
        }
    }
}

fn truncate_utf8(text: &mut String, limit: usize) {
    let mut end = limit.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}

pub fn cleanup(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let now = SystemTime::now();
    let mut files: Vec<(PathBuf, SystemTime, u64)> = entries
        .flatten()
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() || e.path().extension().and_then(|x| x.to_str()) != Some("log") {
                return None;
            }
            Some((e.path(), meta.modified().unwrap_or(UNIX_EPOCH), meta.len()))
        })
        .collect();
    for (path, modified, _) in &files {
        if now.duration_since(*modified).unwrap_or_default() > RETENTION {
            let _ = fs::remove_file(path);
        }
    }
    files.retain(|(p, _, _)| p.exists());
    files.sort_by_key(|(_, modified, _)| *modified);
    let mut regular: u64 = files
        .iter()
        .filter(|(p, _, _)| {
            p.extension().and_then(|x| x.to_str()) == Some("log")
                && !p
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .starts_with("crash-")
        })
        .map(|x| x.2)
        .sum();
    let mut total: u64 = files.iter().map(|x| x.2).sum();
    let mut crash: u64 = files
        .iter()
        .filter(|(p, _, _)| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with("crash-")
        })
        .map(|x| x.2)
        .sum();
    let mut crash_count = files
        .iter()
        .filter(|(p, _, _)| {
            p.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with("crash-")
        })
        .count();
    for (path, _, size) in files {
        let is_crash = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .starts_with("crash-");
        let remove = total > LOG_DIR_LIMIT
            || (!is_crash && regular > REGULAR_LOG_LIMIT)
            || (is_crash && (crash > CRASH_LOG_LIMIT || crash_count > 5));
        if !remove {
            continue;
        }
        if fs::remove_file(&path).is_ok() {
            total = total.saturating_sub(size);
            if is_crash {
                crash = crash.saturating_sub(size);
                crash_count = crash_count.saturating_sub(1);
            } else {
                regular = regular.saturating_sub(size);
            }
        }
    }
}

fn install_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let detail = format!("message={info}");
        error("app", "panic", "-", &detail);
        let dir = log_dir();
        let _ = fs::create_dir_all(&dir);
        let path = dir.join(format!("crash-{}.log", now_ms()));
        let mut text = redact(&detail);
        truncate_utf8(&mut text, 512 * 1024);
        let _ = fs::write(path, text);
        cleanup(&dir);
        previous(info);
    }));
}
