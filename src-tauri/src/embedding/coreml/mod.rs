//! Native GPU graph execution with sparse CPU lookup and mask-aware context buckets.
mod bridge;
mod cache;
mod lookup;
mod weights;
use crate::embedding::{
    domain::{normalize, Precision, DIM, MODEL_REVISION},
    preprocess::Patches,
};
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    collections::{hash_map::Entry, HashMap},
    fmt,
    path::{Path, PathBuf},
};
#[derive(Debug)]
struct ExecutionFailure;
impl fmt::Display for ExecutionFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Native CoreML execution failed")
    }
}
impl std::error::Error for ExecutionFailure {}
pub(super) fn is_inference_failure(error: &anyhow::Error) -> bool {
    error.is::<ExecutionFailure>()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    revision: String,
    buckets: Vec<usize>,
    width: usize,
    dimension: usize,
    image_token: i64,
    video_token: i64,
    audio_token: Option<i64>,
    recipes: HashMap<String, weights::Recipe>,
}
pub(super) struct Native {
    directory: PathBuf,
    handle: bridge::Handle,
    table: lookup::Table,
    metadata: Metadata,
}
pub(super) fn load(root: &Path, precision: Precision) -> Result<Native> {
    let metadata: Metadata = serde_json::from_str(cache::METADATA)?;
    if metadata.revision != MODEL_REVISION
        || metadata.width != 512
        || metadata.dimension != DIM
        || metadata.buckets != cache::BUCKETS
    {
        bail!("Native template metadata does not match the model revision");
    }
    let bridge = bridge::Bridge::shared(root)?;
    let directory = cache::prepare(root, precision, &bridge)?;
    let table = lookup::Table::open(
        root,
        metadata
            .recipes
            .get(precision.key())
            .context("Missing token precision recipe")?,
        metadata.width,
    )?;
    let handle = bridge.open(&directory)?;
    Ok(Native {
        directory,
        handle,
        table,
        metadata,
    })
}
impl Native {
    pub fn vision(&mut self, mut patches: Patches) -> Result<Vec<f32>> {
        if patches.max_patches > 2520
            || patches.soft_tokens > 280
            || patches.pixels.len() != patches.max_patches * 768
            || patches.positions.len() != patches.max_patches * 2
        {
            bail!("Invalid native visual input");
        }
        patches.pixels.resize(2520 * 768, 0.);
        patches.positions.resize(2520 * 2, -1);
        let positions = patches
            .positions
            .into_iter()
            .map(i32::try_from)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut output = self
            .handle
            .vision(&patches.pixels, &positions)
            .context(ExecutionFailure)?;
        output.truncate(patches.soft_tokens * 512);
        if output.iter().any(|x| !x.is_finite()) {
            return Err(anyhow::anyhow!("Non-finite native visual features"))
                .context(ExecutionFailure);
        }
        Ok(output)
    }
    pub fn encode(&mut self, tokens: &[i64], features: &[f32], video: bool) -> Result<Vec<f32>> {
        let input = prepare(&self.metadata, &self.table, tokens, features, video)?;
        let Inputs {
            bucket,
            embedded,
            ids,
            mask,
            positions,
        } = input;
        let values = self
            .handle
            .body(
                &self.directory.join(format!("body-{bucket}.mlmodelc")),
                &embedded,
                &ids,
                &mask,
                &positions,
            )
            .context(ExecutionFailure)?;
        normalize(values).context(ExecutionFailure)
    }
}

struct Inputs {
    bucket: usize,
    embedded: Vec<f32>,
    ids: Vec<i32>,
    mask: Vec<i32>,
    positions: Vec<f32>,
}
fn prepare(
    metadata: &Metadata,
    table: &lookup::Table,
    tokens: &[i64],
    features: &[f32],
    video: bool,
) -> Result<Inputs> {
    if tokens.is_empty()
        || tokens.len() > 8192
        || !features.len().is_multiple_of(512)
        || features.len() / 512 > 4480
    {
        bail!("Invalid native backbone input");
    }
    let bucket = metadata
        .buckets
        .iter()
        .copied()
        .find(|n| *n >= tokens.len())
        .context("Unsupported model context")?;
    let wanted = if video {
        metadata.video_token
    } else {
        metadata.image_token
    };
    let mut used = 0;
    let mut embedded = vec![0.; bucket * 512];
    let mut ids = vec![0; bucket];
    let mut mask = vec![0; bucket];
    let positions: Vec<_> = (0..bucket).map(|x| x as f32).collect();
    let mut rows = HashMap::new();
    for (index, &id) in tokens.iter().enumerate() {
        ids[index] = i32::try_from(id)?;
        mask[index] = 1;
        let values = if id == metadata.image_token
            || id == metadata.video_token
            || metadata.audio_token == Some(id)
        {
            if id != wanted || used + 512 > features.len() {
                bail!("Modality tokens do not match the visual input");
            }
            let values = &features[used..used + 512];
            used += 512;
            values
        } else {
            let row = match rows.entry(id) {
                Entry::Occupied(entry) => entry.into_mut(),
                Entry::Vacant(entry) => entry.insert(table.row(id)?),
            };
            row.as_slice()
        };
        if values.iter().any(|v| !v.is_finite()) {
            return Err(anyhow::anyhow!("Non-finite model input features"))
                .context(ExecutionFailure);
        }
        embedded[index * 512..(index + 1) * 512].copy_from_slice(values);
    }
    if used != features.len() {
        bail!("Visual feature count does not match the model tokens");
    }
    Ok(Inputs {
        bucket,
        embedded,
        ids,
        mask,
        positions,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/coreml_inputs.rs"]
mod tests;
