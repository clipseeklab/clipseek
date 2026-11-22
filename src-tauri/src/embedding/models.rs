use crate::embedding::domain::{Precision, Settings, MODEL_ID, MODEL_REVISION};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha1::Sha1;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelFile {
    path: String,
    size: u64,
    sha256: Option<String>,
    blob_id: Option<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub active: bool,
    pub file: String,
    pub bytes: u64,
    pub total: u64,
    pub error: Option<String>,
}

pub fn directory(settings: &Settings) -> PathBuf {
    Path::new(&settings.model_directory).join(settings.precision.key())
}
fn catalogue(precision: Precision) -> Result<Vec<ModelFile>> {
    let all: Vec<ModelFile> = serde_json::from_str(include_str!("../../model-files.json"))?;
    Ok(all
        .into_iter()
        .filter(|file| {
            !file.path.starts_with("onnx/")
                || ["model", "vision_encoder"].iter().any(|name| {
                    let graph = format!("onnx/{name}{}.onnx", precision.suffix());
                    file.path == graph || file.path == format!("{graph}_data")
                })
        })
        .collect())
}

pub fn available(settings: &Settings) -> bool {
    catalogue(settings.precision).is_ok_and(|files| {
        files.iter().all(|file| {
            fs::metadata(directory(settings).join(&file.path))
                .is_ok_and(|meta| meta.len() == file.size)
        })
    })
}
fn verify_file(root: &Path, file: &ModelFile) -> Result<()> {
    let mut input = File::open(root.join(&file.path))
        .with_context(|| format!("Missing model file {}", file.path))?;
    if input.metadata()?.len() != file.size {
        bail!("Model file size mismatch: {}", file.path);
    }
    let mut sha256 = Sha256::new();
    let mut sha1 = Sha1::new();
    sha1.update(format!("blob {}\0", file.size));
    let mut buffer = [0u8; 65536];
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        sha256.update(&buffer[..count]);
        sha1.update(&buffer[..count]);
    }
    let valid = match (&file.sha256, &file.blob_id) {
        (Some(expected), _) => format!("{:x}", sha256.finalize()) == *expected,
        (_, Some(expected)) => format!("{:x}", sha1.finalize()) == *expected,
        _ => false,
    };
    if !valid {
        bail!("Model file integrity check failed: {}", file.path);
    }
    Ok(())
}
pub fn verify(root: &Path, precision: Precision) -> Result<()> {
    for file in catalogue(precision)? {
        verify_file(root, &file)?;
    }
    Ok(())
}

pub fn download(settings: &Settings, progress: &dyn Fn(Transfer)) -> Result<()> {
    let root = directory(settings);
    fs::create_dir_all(root.join("onnx"))?;
    let files = catalogue(settings.precision)?;
    let total = files.iter().map(|f| f.size).sum();
    let mut completed = 0;
    let client = reqwest::blocking::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(1800))
        .user_agent("clipstudio-open/0.1")
        .build()?;
    for file in files {
        if verify_file(&root, &file).is_ok() {
            completed += file.size;
            continue;
        }
        let destination = root.join(&file.path);
        let temporary = destination.with_extension("part");
        let mut outcome = Err(anyhow::anyhow!("Model download failed"));
        for host in ["https://huggingface.co", "https://hf-mirror.com"] {
            outcome = (|| -> Result<()> {
                let url = format!("{host}/{MODEL_ID}/resolve/{MODEL_REVISION}/{}", file.path);
                let mut response = client.get(url).send()?.error_for_status()?;
                let mut output = File::create(&temporary)?;
                let mut received = 0;
                let mut buffer = [0u8; 65536];
                let mut notified = 0;
                loop {
                    let count = response.read(&mut buffer)?;
                    if count == 0 {
                        break;
                    }
                    received += count as u64;
                    if received > file.size {
                        bail!("Model download exceeds its expected size");
                    }
                    output.write_all(&buffer[..count])?;
                    if received - notified > 1024 * 1024 || received == file.size {
                        progress(Transfer {
                            active: true,
                            file: file.path.clone(),
                            bytes: completed + received,
                            total,
                            error: None,
                        });
                        notified = received;
                    }
                }
                output.sync_all()?;
                drop(output);
                // Check before publishing. Never replace an installed file with unchecked bytes.
                let staged = ModelFile {
                    path: temporary.file_name().unwrap().to_string_lossy().into(),
                    size: file.size,
                    sha256: file.sha256.clone(),
                    blob_id: file.blob_id.clone(),
                };
                verify_file(temporary.parent().unwrap(), &staged)?;
                if destination.exists() {
                    fs::remove_file(&destination)?;
                }
                fs::rename(&temporary, &destination)?;
                Ok(())
            })();
            if outcome.is_ok() {
                break;
            }
        }
        if let Err(error) = outcome {
            let _ = fs::remove_file(temporary);
            return Err(error.context(format!("Failed to download {}", file.path)));
        }
        completed += file.size;
    }
    verify(&root, settings.precision)?;
    progress(Transfer {
        active: false,
        bytes: total,
        total,
        ..Default::default()
    });
    Ok(())
}
