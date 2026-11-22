//! Atomic, integrity-checked compilation with shared weights across context buckets.
use super::{
    bridge::Bridge,
    weights::{self, Recipe},
};
use crate::embedding::domain::Precision;
use anyhow::{bail, Context, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use fs2::FileExt;
use half::f16;
use serde::Deserialize;
use std::{
    collections::HashMap,
    fs,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};
pub(super) const BUCKETS: &[usize] = &[32, 128, 256, 512, 1024, 2048, 2560, 4096, 4608, 8192];
pub(super) const METADATA: &str = include_str!("../../../native/coreml/metadata.json");
const VISION: &[u8] = include_bytes!("../../../native/coreml/vision.mlmodel");
const VISION_LAYOUT: &str = include_str!("../../../native/coreml/vision-layout.json");
const BODY_LAYOUT: &str = include_str!("../../../native/coreml/body-layout.json");
const BODIES: &[&[u8]] = &[
    include_bytes!("../../../native/coreml/body-32.mlmodel"),
    include_bytes!("../../../native/coreml/body-128.mlmodel"),
    include_bytes!("../../../native/coreml/body-256.mlmodel"),
    include_bytes!("../../../native/coreml/body-512.mlmodel"),
    include_bytes!("../../../native/coreml/body-1024.mlmodel"),
    include_bytes!("../../../native/coreml/body-2048.mlmodel"),
    include_bytes!("../../../native/coreml/body-2560.mlmodel"),
    include_bytes!("../../../native/coreml/body-4096.mlmodel"),
    include_bytes!("../../../native/coreml/body-4608.mlmodel"),
    include_bytes!("../../../native/coreml/body-8192.mlmodel"),
];
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Layout {
    file_size: u64,
    headers: Vec<Header>,
    segments: Vec<Segment>,
}
#[derive(Deserialize)]
struct Header {
    offset: u64,
    data: String,
}
#[derive(Deserialize)]
struct Segment {
    offset: u64,
    dtype: String,
    count: usize,
    source: Source,
}
#[derive(Deserialize)]
struct Source {
    recipes: Option<HashMap<String, Recipe>>,
    transpose: Option<bool>,
    fill: Option<f32>,
    range: Option<bool>,
}
fn segment(root: &Path, entry: &Segment, precision: Precision) -> Result<Vec<u8>> {
    let source = &entry.source;
    let values = if let Some(recipes) = &source.recipes {
        weights::values(
            root,
            recipes
                .get(precision.key())
                .context("Missing precision recipe")?,
        )?
    } else if let Some(fill) = source.fill {
        vec![fill; entry.count]
    } else if source.range == Some(true) {
        (0..entry.count).map(|x| x as f32).collect()
    } else {
        bail!("Missing native weight recipe");
    };
    if values.len() != entry.count || source.transpose == Some(true) {
        bail!("Unexpected native weight layout");
    }
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        if !value.is_finite() {
            bail!("Non-finite native source weight");
        }
        match entry.dtype.as_str() {
            "float16" => bytes.extend(f16::from_f32(value.clamp(-65504., 65504.)).to_le_bytes()),
            "float32" => bytes.extend(value.to_le_bytes()),
            "int32" => bytes.extend((value as i32).to_le_bytes()),
            "uint16" => bytes.extend((value as u16).to_le_bytes()),
            _ => bail!("Unsupported native weight type"),
        }
    }
    Ok(bytes)
}
fn write_weights(root: &Path, destination: &Path, text: &str, precision: Precision) -> Result<()> {
    let layout: Layout = serde_json::from_str(text)?;
    let mut file = fs::File::create(destination)?;
    file.set_len(layout.file_size)?;
    for header in layout.headers {
        let data = STANDARD.decode(header.data)?;
        if data.len() != 64
            || header.offset.checked_add(64).context("Header overflow")? > layout.file_size
        {
            bail!("Invalid native weight header");
        }
        file.seek(SeekFrom::Start(header.offset))?;
        file.write_all(&data)?;
    }
    for entry in layout.segments {
        let bytes = segment(root, &entry, precision)?;
        if entry
            .offset
            .checked_add(bytes.len() as u64)
            .context("Native data overflow")?
            > layout.file_size
        {
            bail!("Invalid native weight output range");
        }
        file.seek(SeekFrom::Start(entry.offset))?;
        file.write_all(&bytes)?;
    }
    file.sync_all()?;
    Ok(())
}
fn manifest() -> Result<Vec<u8>> {
    let model = uuid::Uuid::new_v4().to_string();
    let weights = uuid::Uuid::new_v4().to_string();
    Ok(serde_json::to_vec(
        &serde_json::json!({"fileFormatVersion":"1.0.0","rootModelIdentifier":model,"itemInfoEntries":{
  (model.clone()):{"author":"com.apple.CoreML","description":"CoreML Model Specification","name":"model.mlmodel","path":"com.apple.CoreML/model.mlmodel"},
  (weights):{"author":"com.apple.CoreML","description":"CoreML Model Weights","name":"weights","path":"com.apple.CoreML/weights"}}}),
    )?)
}
fn hash(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}
fn package(destination: &Path, graph: &[u8], weights: &Path) -> Result<()> {
    let directory = destination.join("Data/com.apple.CoreML");
    fs::create_dir_all(directory.join("weights"))?;
    fs::write(destination.join("Manifest.json"), manifest()?)?;
    fs::write(directory.join("model.mlmodel"), graph)?;
    fs::hard_link(weights, directory.join("weights/weight.bin"))
        .or_else(|_| fs::copy(weights, directory.join("weights/weight.bin")).map(|_| ()))?;
    Ok(())
}
fn digest(directory: &Path) -> Result<HashMap<String, String>> {
    let mut hashes = HashMap::new();
    let mut identities = HashMap::new();
    for entry in walkdir::WalkDir::new(directory).min_depth(1) {
        let entry = entry?;
        if entry.file_type().is_symlink() {
            bail!("Unexpected native cache symlink");
        }
        if !entry.file_type().is_file() || entry.path() == directory.join("integrity.json") {
            continue;
        }
        let meta = entry.metadata()?;
        let identity = (meta.dev(), meta.ino(), meta.len());
        let value = if let Some(hash) = identities.get(&identity) {
            String::clone(hash)
        } else {
            let hash = hash(entry.path())?;
            identities.insert(identity, hash.clone());
            hash
        };
        let name = entry
            .path()
            .strip_prefix(directory)?
            .to_str()
            .context("Invalid native cache name")?
            .to_owned();
        hashes.insert(name, value);
    }
    Ok(hashes)
}
fn share_compiled(weights: &Path, compiled: &Path, expected: &str) -> Result<()> {
    let file = compiled.join("weights/weight.bin");
    if !file.exists() || hash(&file)? != expected {
        return Ok(());
    }
    let temporary = compiled.join("shared-weights.tmp");
    if fs::hard_link(weights, &temporary).is_ok() {
        fs::rename(temporary, file)?;
    }
    Ok(())
}
pub(super) fn prepare(root: &Path, precision: Precision, bridge: &Bridge) -> Result<PathBuf> {
    let mut hasher = blake3::Hasher::new();
    for bytes in [
        VISION,
        VISION_LAYOUT.as_bytes(),
        BODY_LAYOUT.as_bytes(),
        METADATA.as_bytes(),
    ] {
        hasher.update(bytes);
    }
    for bytes in BODIES {
        hasher.update(bytes);
    }
    let directory = root.join(format!(
        "coreml-gpu-v2-{}-{}",
        precision.key(),
        &hasher.finalize().to_hex()[..16]
    ));
    let lock = fs::File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root.join(".coreml-v2.lock"))?;
    FileExt::lock_exclusive(&lock)?;
    if directory.exists() {
        let valid = (|| -> Result<bool> {
            let expected: HashMap<String, String> =
                serde_json::from_slice(&fs::read(directory.join("integrity.json"))?)?;
            Ok(expected.len() > BUCKETS.len() && expected == digest(&directory)?)
        })()
        .unwrap_or(false);
        if valid {
            return Ok(directory);
        }
        fs::remove_dir_all(&directory)?;
    }
    let stage = tempfile::Builder::new()
        .prefix(".coreml-v2-build-")
        .tempdir_in(root)?;
    let body_weights = stage.path().join("body-weights.bin");
    write_weights(root, &body_weights, BODY_LAYOUT, precision)?;
    let body_hash = hash(&body_weights)?;
    for (index, bucket) in BUCKETS.iter().enumerate() {
        let source = stage.path().join(format!("body-{bucket}.mlpackage"));
        let compiled = stage.path().join(format!("body-{bucket}.mlmodelc"));
        package(&source, BODIES[index], &body_weights)?;
        bridge.compile(&source, &compiled)?;
        share_compiled(&body_weights, &compiled, &body_hash)?;
        fs::remove_dir_all(source)?;
    }
    let vision_weights = stage.path().join("vision-weights.bin");
    write_weights(root, &vision_weights, VISION_LAYOUT, precision)?;
    let source = stage.path().join("vision.mlpackage");
    package(&source, VISION, &vision_weights)?;
    bridge.compile(&source, &stage.path().join("vision.mlmodelc"))?;
    fs::remove_dir_all(source)?;
    fs::remove_file(vision_weights)?;
    fs::write(
        stage.path().join("integrity.json"),
        serde_json::to_vec(&digest(stage.path())?)?,
    )?;
    fs::rename(stage.path(), &directory)?;
    Ok(directory)
}

#[cfg(test)]
#[path = "../../../tests/unit/coreml_cache.rs"]
mod tests;
