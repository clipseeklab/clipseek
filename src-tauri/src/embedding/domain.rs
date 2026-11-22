use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

pub const DIM: usize = 768;
pub const MAX_VIDEO_SECONDS: f64 = 32.0;
pub const MODEL_ID: &str = "onnx-community/embeddinggemma-2-ONNX";
pub const MODEL_REVISION: &str = "da4d2517b6a48d3de6d480ac2126ef5353c30713";

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Precision {
    Q4,
    #[default]
    Q8,
    Fp32,
}
impl Precision {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::Q4 => "_q4",
            Self::Q8 => "_quantized",
            Self::Fp32 => "",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Q4 => "q4",
            Self::Q8 => "q8",
            Self::Fp32 => "fp32",
        }
    }
    pub fn profile(self) -> String {
        format!("gemma2-{MODEL_REVISION}-{}-v1", self.key())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub precision: Precision,
    pub model_directory: String,
}

pub fn validate_duration(seconds: f64) -> Result<()> {
    if !seconds.is_finite() || seconds <= 0.0 {
        bail!("Could not determine a valid video duration");
    }
    if seconds > MAX_VIDEO_SECONDS {
        bail!("Video duration {seconds:.3} seconds exceeds the 32-second limit");
    }
    Ok(())
}

pub fn normalize(mut values: Vec<f32>) -> Result<Vec<f32>> {
    if values.len() != DIM || values.iter().any(|v| !v.is_finite()) {
        bail!("Model returned an invalid 768-dimensional vector");
    }
    let norm = values.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !norm.is_finite() || norm <= 1e-12 {
        bail!("Model returned a zero vector");
    }
    values.iter_mut().for_each(|v| *v /= norm);
    Ok(values)
}
