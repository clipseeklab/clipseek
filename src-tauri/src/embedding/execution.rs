//! Native inference placement. Apple Silicon prefers native CoreML GPU with Metal and CPU fallbacks.
use anyhow::{bail, Result};
use ort::session::Session;
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BackendPreference {
    #[default]
    Auto,
    Cpu,
    Metal,
    Coreml,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Cpu,
    Metal,
    Coreml,
}
impl Backend {
    pub fn label(self) -> &'static str {
        match self {
            Self::Cpu => "CPU",
            Self::Metal => "Metal GPU (WebGPU)",
            Self::Coreml => "CoreML GPU",
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionStatus {
    pub backend: Backend,
    pub fallback_reason: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct ExecutionOptions {
    pub preference: BackendPreference,
    pub profile_directory: Option<PathBuf>,
}

pub fn preferred_backend(preference: BackendPreference) -> Result<Backend> {
    match preference {
        BackendPreference::Cpu => Ok(Backend::Cpu),
        BackendPreference::Auto if cfg!(all(target_os = "macos", target_arch = "aarch64")) => {
            Ok(Backend::Coreml)
        }
        BackendPreference::Auto => Ok(Backend::Cpu),
        BackendPreference::Metal if cfg!(all(target_os = "macos", target_arch = "aarch64")) => {
            Ok(Backend::Metal)
        }
        BackendPreference::Coreml if cfg!(all(target_os = "macos", target_arch = "aarch64")) => {
            Ok(Backend::Coreml)
        }
        BackendPreference::Coreml => bail!("Native CoreML GPU requires an Apple Silicon Mac"),
        BackendPreference::Metal => bail!("Metal acceleration requires an Apple Silicon Mac"),
    }
}

pub fn session(path: &Path, backend: Backend, profile: Option<&Path>) -> Result<Session> {
    if backend == Backend::Coreml {
        bail!("Native CoreML does not use ONNX sessions");
    }
    // Bound Windows CPU workers alongside the second encoder session and desktop.
    let maximum_threads = if cfg!(target_os = "windows") { 4 } else { 8 };
    let mut builder = Session::builder()?
        .with_intra_threads(
            std::thread::available_parallelism().map_or(4, |n| n.get().min(maximum_threads)),
        )
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    if let Some(profile) = profile {
        builder = builder
            .with_profiling(profile)
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
    }
    if backend == Backend::Metal {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            use ort::ep::{ArbitrarilyConfigurableExecutionProvider, ExecutionProvider};
            let gpu = ort::ep::WebGPU::default()
                .with_arbitrary_config("ep.webgpuexecutionprovider.dawnBackendType", "Metal");
            if !gpu.is_available()? {
                bail!("The bundled ONNX Runtime does not include WebGPU acceleration");
            }
            // Registration must fail explicitly; never report Metal after a silent EP fallback.
            builder = builder
                .with_execution_providers([gpu.build().error_on_failure()])
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
        }
        #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
        bail!("Metal acceleration requires an Apple Silicon Mac");
    }
    let expanded = if backend == Backend::Metal
        && path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with("_quantized.onnx"))
    {
        Some(super::q8_metal::expand(path)?)
    } else {
        None
    };
    Ok(builder.commit_from_file(expanded.as_ref().map_or(path, |model| model.path.as_path()))?)
}
