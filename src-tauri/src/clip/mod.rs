//! EmbeddingGemma 2 adapter behind the established shared encoder boundary.
mod handle;
mod resident;
use crate::embedding::{domain::Settings, engine::Engine, media::Media};
use crate::error::{AppError, AppResult};
pub use handle::{warm_search_in_background, ClipHandle};
use image::DynamicImage;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex, RwLock,
    },
};
pub const EMBED_DIM: usize = 768;
const IMAGE_SIZE: u32 = 48;
pub fn resolve_models_dir() -> Option<PathBuf> {
    let settings = crate::model_download::settings();
    crate::embedding::models::available(&settings)
        .then(|| crate::embedding::models::directory(&settings))
}
pub struct ClipEncoder {
    engine: Mutex<Engine>,
    visual_ready: AtomicBool,
    execution: RwLock<crate::embedding::execution::ExecutionStatus>,
}
impl ClipEncoder {
    pub fn load(_path: &Path) -> AppResult<Self> {
        let settings = crate::model_download::settings();
        Self::load_settings(&settings)
    }
    pub fn load_settings(settings: &Settings) -> AppResult<Self> {
        crate::embedding::engine::initialize(&crate::model_download::runtime())
            .map_err(|e| AppError::Msg(format!("{e:#}")))?;
        let engine = Engine::load(settings).map_err(|e| AppError::Msg(format!("{e:#}")))?;
        let execution = engine.execution.clone();
        Ok(Self {
            engine: Mutex::new(engine),
            visual_ready: AtomicBool::new(false),
            execution: RwLock::new(execution),
        })
    }
    pub fn encode_texts(&self, texts: &[&str]) -> AppResult<Vec<Vec<f32>>> {
        let mut engine = self.engine.lock().unwrap();
        let result = texts
            .iter()
            .map(|text| {
                engine
                    .query(text)
                    .map_err(|e| AppError::Msg(format!("{e:#}")))
            })
            .collect();
        *self.execution.write().unwrap() = engine.execution.clone();
        result
    }
    pub fn encode_images(&self, images: &[DynamicImage]) -> AppResult<Vec<Vec<f32>>> {
        let mut engine = self.engine.lock().unwrap();
        let result: AppResult<Vec<Vec<f32>>> = images
            .iter()
            .map(|image| {
                engine
                    .image(image)
                    .map_err(|e| AppError::Msg(format!("{e:#}")))
            })
            .collect();
        *self.execution.write().unwrap() = engine.execution.clone();
        if result.is_ok() && !images.is_empty() {
            self.visual_ready.store(true, Ordering::Release);
        }
        result
    }
    pub fn encode_asset(
        &self,
        path: &Path,
        video: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> AppResult<Vec<f32>> {
        let runtime = crate::model_download::runtime();
        let ext = if cfg!(windows) { ".exe" } else { "" };
        let media = Media {
            ffmpeg: runtime.join(format!("ffmpeg{ext}")),
            ffprobe: runtime.join(format!("ffprobe{ext}")),
        };
        let mut engine = self.engine.lock().unwrap();
        let result = engine
            .asset(path, video, &media, &|| !cancelled())
            .map_err(|e| AppError::Msg(format!("{e:#}")));
        *self.execution.write().unwrap() = engine.execution.clone();
        if result.is_ok() {
            self.visual_ready.store(true, Ordering::Release);
        }
        result
    }
    pub fn execution_status(&self) -> crate::embedding::execution::ExecutionStatus {
        self.execution.read().unwrap().clone()
    }
    pub fn visual_provider(&self) -> String {
        self.execution_status().backend.label().into()
    }
}
pub fn l2_normalize(values: &mut [f32]) {
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0. {
        for value in values {
            *value /= norm;
        }
    }
}
