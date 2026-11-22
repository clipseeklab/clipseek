//! Shared encoder initialization, query cache and background warmup.

use super::{resident::ModelCache, resolve_models_dir, ClipEncoder, IMAGE_SIZE};
use crate::error::{AppError, AppResult};
use image::DynamicImage;
use std::sync::Arc;
use std::time::{Duration, Instant};

const IDLE_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(30);

/// Load and share the encoder on demand; allow idle unloading after all tasks release it.
#[derive(Default)]
pub struct ClipHandle {
    inner: Arc<ModelCache<ClipEncoder>>,
    warmup_running: std::sync::atomic::AtomicBool,
    generation: std::sync::atomic::AtomicU64,
    text_cache: std::sync::Mutex<std::collections::HashMap<String, Vec<f32>>>,
}

impl ClipHandle {
    /// Nonblocking status: initialized means a complete visual inference succeeded.
    pub fn visual_ready(&self) -> bool {
        self.inner
            .inspect(|clip| clip.visual_ready.load(std::sync::atomic::Ordering::Acquire))
            .unwrap_or(false)
    }

    /// Report the loaded backend without initializing or keeping the encoder resident.
    pub fn execution_status(&self) -> Option<crate::embedding::execution::ExecutionStatus> {
        self.inner.inspect(|clip| clip.execution_status())
    }

    /// Concurrent search, indexing, and warmup share text and vision sessions.
    pub fn get_or_init(&self) -> AppResult<std::sync::Arc<ClipEncoder>> {
        self.inner.get_or_try_init(Self::load)
    }

    pub fn invalidate(&self) {
        self.inner.clear();
        self.generation
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        self.text_cache.lock().unwrap().clear();
    }
    fn load() -> AppResult<ClipEncoder> {
        let dir = resolve_models_dir().ok_or_else(|| {
            AppError::coded(
                "model_files_missing",
                "EmbeddingGemma 2 model files are missing; download the selected precision",
            )
        })?;
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ClipEncoder::load(&dir))).map_err(
            |panic| {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|value| (*value).to_string())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown ONNX Runtime panic".into());
                AppError::coded(
                    "model_runtime_init_failed",
                    format!("ONNX Runtime initialization panicked: {message}"),
                )
            },
        )?
    }

    /// The worker owns only a weak cache reference, so it cannot keep models alive on shutdown.
    pub(crate) fn start_idle_cleanup(&self) {
        let cache = Arc::downgrade(&self.inner);
        std::thread::spawn(move || loop {
            std::thread::sleep(IDLE_CHECK_INTERVAL);
            let Some(cache) = cache.upgrade() else { break };
            if cache.release_if_idle(Instant::now(), IDLE_TIMEOUT) {
                crate::logging::info("search", "models_idle_unloaded", "-", "idle_seconds=300");
            }
        });
    }

    /// Cache recent text embeddings so filter changes do not rerun the text model.
    pub fn encode_text_cached(&self, query: &str) -> AppResult<Vec<f32>> {
        const MAX_CACHED_QUERIES: usize = 32;
        loop {
            let generation = self.generation.load(std::sync::atomic::Ordering::Acquire);
            let key = query.trim().to_string();
            if let Some(vector) = self.text_cache.lock().unwrap().get(&key).cloned() {
                return Ok(vector);
            }
            let vector = self
                .get_or_init()?
                .encode_texts(&[&key])?
                .into_iter()
                .next()
                .ok_or_else(|| AppError::Msg("empty encode result".into()))?;
            let mut cache = self.text_cache.lock().unwrap();
            if generation != self.generation.load(std::sync::atomic::Ordering::Acquire) {
                continue;
            }
            if cache.len() >= MAX_CACHED_QUERIES {
                cache.clear();
            }
            cache.insert(key, vector.clone());
            return Ok(vector);
        }
    }
}

/// Prepare the same text/visual sessions used by search and indexing, off the UI thread.
/// A synthetic image warms the same inference sessions before media is imported.
pub fn warm_search_in_background<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    use std::sync::atomic::Ordering;
    use tauri::{Emitter, Manager};
    let state = app.state::<crate::AppState>();
    if state.clip.inner.was_evicted()
        || state.clip.visual_ready()
        || state
            .clip
            .warmup_running
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
    {
        return false;
    }
    let app = app.clone();
    std::thread::spawn(move || {
        let start = std::time::Instant::now();
        let handle = &app.state::<crate::AppState>().clip;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let Some(clip) = handle.inner.get_for_warmup(ClipHandle::load)? else {
                return Ok(false);
            };
            clip.encode_texts(&["media"])?;
            if !handle.visual_ready() {
                clip.encode_images(&[DynamicImage::new_rgb8(IMAGE_SIZE, IMAGE_SIZE)])?;
            }
            Ok::<_, AppError>(true)
        }))
        .unwrap_or_else(|_| {
            Err(AppError::coded(
                "model_runtime_init_failed",
                "Search model warmup panicked",
            ))
        });
        // Failure is retryable on the next model check or actual index operation.
        handle.warmup_running.store(false, Ordering::Release);
        if matches!(result, Ok(false)) {
            return;
        }
        let _ = app.emit("model://warmup", serde_json::json!({ "ready": result.is_ok(), "error": result.as_ref().err().map(ToString::to_string) }));
        match result {
            Ok(_) => crate::logging::info(
                "search",
                "model_warmup",
                "-",
                &format!("elapsed_ms={}", start.elapsed().as_millis()),
            ),
            Err(e) => crate::logging::warn(
                "search",
                "model_warmup_deferred",
                "-",
                &format!("elapsed_ms={};message={e}", start.elapsed().as_millis()),
            ),
        }
    });
    true
}

#[cfg(test)]
#[path = "../../tests/unit/clip_handle.rs"]
mod tests;
