use crate::embedding::{
    domain::{normalize, Settings},
    execution::{self, Backend, BackendPreference, ExecutionOptions, ExecutionStatus},
    media::Media,
    models, preprocess,
};
use anyhow::{bail, Context, Result};
use ort::{session::Session, value::Tensor};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

struct OnnxSessions {
    text: Session,
    vision: Session,
}
enum Inference {
    Onnx(Box<OnnxSessions>),
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    Coreml(Box<super::coreml::Native>),
}
pub struct Engine {
    inference: Inference,
    root: PathBuf,
    precision: super::domain::Precision,
    options: ExecutionOptions,
    tokenizer: Tokenizer,
    pub identity: String,
    pub execution: ExecutionStatus,
}

impl Engine {
    pub fn load(settings: &Settings) -> Result<Self> {
        Self::load_with_options(settings, &ExecutionOptions::default())
    }
    pub fn load_with_options(settings: &Settings, options: &ExecutionOptions) -> Result<Self> {
        let root = models::directory(settings);
        models::verify(&root, settings.precision)?;
        let preferred = execution::preferred_backend(options.preference)?;
        let (inference, execution) =
            load_with_fallback(&root, settings.precision, options, preferred)?;
        let tokenizer = Tokenizer::from_file(root.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(Self {
            inference,
            root,
            precision: settings.precision,
            options: options.clone(),
            execution,
            tokenizer,
            identity: identity(settings),
        })
    }
    pub fn end_profiling(&mut self) -> Result<[PathBuf; 2]> {
        match &mut self.inference {
            Inference::Onnx(sessions) => Ok([
                sessions.text.end_profiling()?.into(),
                sessions.vision.end_profiling()?.into(),
            ]),
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            Inference::Coreml(_) => bail!("Use the CoreML compute plan for native profiling"),
        }
    }
    pub fn query(&mut self, query: &str) -> Result<Vec<f32>> {
        let query = query.trim();
        if query.is_empty() {
            bail!("Enter a search query");
        }
        let encoding = self
            .tokenizer
            .encode(format!("task: search result | query: {query}"), true)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if encoding.len() > 8192 {
            bail!("Search query exceeds the model context limit");
        }
        let ids: Vec<i64> = encoding.get_ids().iter().map(|id| *id as i64).collect();
        self.recover(|engine| engine.encode(ids.clone(), vec![], false))
    }
    pub fn image(&mut self, image: &image::DynamicImage) -> Result<Vec<f32>> {
        self.recover(|engine| {
            let patches = preprocess::patchify(&image.to_rgb8(), 280)?;
            let ids = preprocess::media_tokens(&[patches.soft_tokens], false);
            let features = engine.vision(patches)?;
            engine.encode(ids, features, false)
        })
    }
    pub fn asset(
        &mut self,
        path: &Path,
        video: bool,
        media: &Media,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>> {
        self.recover(|engine| engine.asset_inner(path, video, media, should_continue))
    }
    fn asset_inner(
        &mut self,
        path: &Path,
        video: bool,
        media: &Media,
        should_continue: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>> {
        let mut features = Vec::new();
        let mut counts = Vec::new();
        if video {
            let (duration, original_width, original_height) = media.video_info(path)?;
            // One model input for the entire <=32-second clip. No persisted frames.
            let count = (duration.floor() as usize).clamp(1, 32);
            let (width, height) = preprocess::target_size(original_width, original_height, 140)?;
            for index in 0..count {
                if !should_continue() {
                    bail!("Indexing cancelled or configuration changed");
                }
                let rgb = media.model_frame(path, index as f64, width, height)?;
                let patches = preprocess::patchify_resized(&rgb, 140)?;
                counts.push(patches.soft_tokens);
                features.extend(self.vision(patches)?);
            }
        } else {
            let rgb = crate::import::thumb::open_image_smart(path)?.to_rgb8();
            let patches = preprocess::patchify(&rgb, 280)?;
            counts.push(patches.soft_tokens);
            features = self.vision(patches)?;
        }
        if !should_continue() {
            bail!("Indexing cancelled or configuration changed");
        }
        self.encode(preprocess::media_tokens(&counts, video), features, video)
    }
    fn recover<T>(&mut self, mut run: impl FnMut(&mut Self) -> Result<T>) -> Result<T> {
        match run(self) {
            Err(error)
                if self.execution.backend == Backend::Coreml
                    && self.options.preference == BackendPreference::Auto
                    && native_failure(&error) =>
            {
                let (inference, mut execution) =
                    load_with_fallback(&self.root, self.precision, &self.options, Backend::Metal)?;
                let reason = format!("Native CoreML inference failed: {error:#}");
                execution.fallback_reason = Some(match execution.fallback_reason {
                    Some(other) => format!("{reason}; {other}"),
                    None => reason,
                });
                self.inference = inference;
                self.execution = execution;
                // Retry the complete media pass, never mixing visual features from two providers.
                run(self)
            }
            result => result,
        }
    }
    fn vision(&mut self, patches: preprocess::Patches) -> Result<Vec<f32>> {
        let vision = match &mut self.inference {
            Inference::Onnx(sessions) => &mut sessions.vision,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            Inference::Coreml(native) => return native.vision(patches),
        };
        let outputs = vision.run(ort::inputs![
            "pixel_values" => Tensor::from_array(([1, patches.max_patches, 768], patches.pixels))?,
            "pixel_position_ids" => Tensor::from_array(([1, patches.max_patches, 2], patches.positions))?
        ])?;
        let (_, values) = outputs["image_features"].try_extract_tensor::<f32>()?;
        if values.len() != patches.soft_tokens * 512 {
            bail!("Vision encoder output does not match the token count");
        }
        Ok(values.to_vec())
    }
    fn encode(&mut self, ids: Vec<i64>, features: Vec<f32>, video: bool) -> Result<Vec<f32>> {
        if ids.len() > 8192 {
            bail!("Media exceeds the model token limit");
        }
        let text = match &mut self.inference {
            Inference::Onnx(sessions) => &mut sessions.text,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            Inference::Coreml(native) => return native.encode(&ids, &features, video),
        };
        let count = features.len() / 512;
        let empty = || Tensor::from_array(([0, 512], Vec::<f32>::new()));
        let feature_tensor = Tensor::from_array(([count, 512], features))?;
        let (image, video_features) = if video {
            (empty()?, feature_tensor)
        } else {
            (feature_tensor, empty()?)
        };
        let n = ids.len();
        let outputs = text.run(ort::inputs![
            "input_ids" => Tensor::from_array(([1, n], ids))?,
            "attention_mask" => Tensor::from_array(([1, n], vec![1_i64; n]))?,
            "image_features" => image,
            "video_features" => video_features,
            "audio_features" => empty()?
        ])?;
        let (_, values) = outputs["sentence_embedding"].try_extract_tensor::<f32>()?;
        normalize(values.to_vec())
    }
}

fn native_failure(error: &anyhow::Error) -> bool {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        super::coreml::is_inference_failure(error)
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    {
        let _ = error;
        false
    }
}
fn load_inference(
    root: &Path,
    precision: super::domain::Precision,
    options: &ExecutionOptions,
    backend: Backend,
) -> Result<Inference> {
    if backend == Backend::Coreml {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return Ok(Inference::Coreml(Box::new(super::coreml::load(
                root, precision,
            )?)));
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        bail!("Native CoreML requires Apple Silicon");
    }
    let session = |name: &str| {
        let profile = options
            .profile_directory
            .as_ref()
            .map(|directory| directory.join(name));
        execution::session(
            &root
                .join("onnx")
                .join(format!("{name}{}.onnx", precision.suffix())),
            backend,
            profile.as_deref(),
        )
    };
    Ok(Inference::Onnx(Box::new(OnnxSessions {
        text: session("model")?,
        vision: session("vision_encoder")?,
    })))
}
fn load_with_fallback(
    root: &Path,
    precision: super::domain::Precision,
    options: &ExecutionOptions,
    preferred: Backend,
) -> Result<(Inference, ExecutionStatus)> {
    let candidates = if options.preference == BackendPreference::Auto {
        match preferred {
            Backend::Coreml => vec![Backend::Coreml, Backend::Metal, Backend::Cpu],
            Backend::Metal => vec![Backend::Metal, Backend::Cpu],
            Backend::Cpu => vec![Backend::Cpu],
        }
    } else {
        vec![preferred]
    };
    let mut failures = Vec::new();
    for backend in candidates {
        match load_inference(root, precision, options, backend) {
            Ok(inference) => {
                return Ok((
                    inference,
                    ExecutionStatus {
                        backend,
                        fallback_reason: (!failures.is_empty()).then(|| failures.join("; ")),
                    },
                ))
            }
            Err(error) => {
                let reason = format!("{} initialization failed: {error:#}", backend.label());
                crate::logging::warn("model", "initialization_failed", "-", &reason);
                failures.push(reason);
            }
        }
    }
    bail!("{}", failures.join("; "))
}

pub fn identity(settings: &Settings) -> String {
    format!(
        "{}:{}",
        settings.precision.profile(),
        settings.model_directory
    )
}
pub fn initialize(runtime: &Path) -> Result<()> {
    let filename = if cfg!(target_os = "windows") {
        "onnxruntime.dll"
    } else if cfg!(target_os = "macos") {
        "libonnxruntime.dylib"
    } else {
        "libonnxruntime.so"
    };
    let path = std::env::var_os("ORT_DYLIB_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(|| runtime.join(filename));
    ort::init_from(&path)
        .with_context(|| {
            format!(
                "Could not load ONNX Runtime: {}. Run npm run runtime:prepare first",
                path.display()
            )
        })?
        .with_name("clipstudio-open")
        .with_telemetry(false)
        .commit();
    Ok(())
}
