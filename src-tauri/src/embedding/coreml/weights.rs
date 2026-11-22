//! Read only pinned ONNX spans; model verification precedes every native load.
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path},
};
#[derive(Clone, Deserialize)]
pub(super) struct Span {
    pub file: String,
    pub offset: u64,
    pub bytes: usize,
    pub dtype: String,
    pub shape: Vec<usize>,
}
#[derive(Clone, Deserialize)]
pub(super) struct Recipe {
    pub kind: String,
    pub data: Span,
    pub scales: Option<Span>,
    pub zeros: Option<Span>,
    pub bits: Option<usize>,
    pub k: Option<usize>,
    pub n: Option<usize>,
    pub block: Option<usize>,
}
pub(super) fn source_path(root: &Path, name: &str) -> Result<std::path::PathBuf> {
    let mut parts = Path::new(name).components();
    if !matches!(
        (parts.next(), parts.next()),
        (Some(Component::Normal(_)), None)
    ) {
        bail!("Invalid native weight source path");
    }
    Ok(root.join("onnx").join(name))
}
pub(super) fn read(root: &Path, span: &Span) -> Result<Vec<u8>> {
    let mut file = fs::File::open(source_path(root, &span.file)?)?;
    if span
        .offset
        .checked_add(span.bytes as u64)
        .context("Weight range overflow")?
        > file.metadata()?.len()
    {
        bail!("Invalid native weight range");
    }
    file.seek(SeekFrom::Start(span.offset))?;
    let mut bytes = vec![0; span.bytes];
    file.read_exact(&mut bytes)?;
    Ok(bytes)
}
pub(super) fn plain(bytes: &[u8], dtype: &str) -> Result<Vec<f32>> {
    let size = match dtype {
        "float32" | "int32" => 4,
        "int64" => 8,
        "uint8" => 1,
        _ => bail!("Unsupported source weight type"),
    };
    if !bytes.len().is_multiple_of(size) {
        bail!("Truncated native source tensor");
    }
    bytes
        .chunks_exact(size)
        .map(|b| {
            Ok(match dtype {
                "float32" => f32::from_le_bytes(b.try_into()?),
                "int32" => i32::from_le_bytes(b.try_into()?) as f32,
                "int64" => {
                    i64::from_le_bytes(b.try_into()?).clamp(i32::MIN as i64, i32::MAX as i64) as f32
                }
                _ => b[0] as f32,
            })
        })
        .collect()
}
pub(super) fn quantized(bytes: &[u8], index: usize, bits: usize) -> f32 {
    if bits == 8 {
        bytes[index] as f32
    } else {
        ((bytes[index / 2] >> ((index % 2) * 4)) & 15) as f32
    }
}
pub(super) fn values(root: &Path, recipe: &Recipe) -> Result<Vec<f32>> {
    let data = read(root, &recipe.data)?;
    if recipe.kind == "plain" {
        return plain(&data, &recipe.data.dtype);
    }
    let bits = recipe.bits.context("Missing quantization bits")?;
    let block = recipe.block.context("Missing quantization block")?;
    if ![4, 8].contains(&bits) || block == 0 {
        bail!("Unsupported native quantization layout");
    }
    let scales = recipe.scales.as_ref().context("Missing scales")?;
    let zeros = recipe.zeros.as_ref().context("Missing zero points")?;
    let s = plain(&read(root, scales)?, &scales.dtype)?;
    let z = read(root, zeros)?;
    let (rows, cols) = if recipe.kind == "matmul" {
        (
            recipe.n.context("Missing N")?,
            recipe.k.context("Missing K")?,
        )
    } else if recipe.kind == "gather" && recipe.data.shape.len() == 2 {
        (recipe.data.shape[0], recipe.data.shape[1] * 8 / bits)
    } else {
        bail!("Unsupported native quantized operator");
    };
    if rows == 0 || cols == 0 {
        bail!("Invalid native quantized dimensions");
    }
    let groups = cols.div_ceil(block);
    let stored = if recipe.kind == "matmul" {
        groups * block
    } else {
        cols
    };
    let zero_stride = groups.div_ceil(8 / bits) * (8 / bits);
    if s.len() != rows.checked_mul(groups).context("Scale shape overflow")?
        || data.len() * 8 / bits != rows.checked_mul(stored).context("Weight shape overflow")?
        || z.len() * 8 / bits
            != rows
                .checked_mul(zero_stride)
                .context("Zero-point shape overflow")?
    {
        bail!("Native quantized tensor size mismatch");
    }
    let mut out = vec![0.; rows.checked_mul(cols).context("Native matrix overflow")?];
    for row in 0..rows {
        for col in 0..cols {
            let group = col / block;
            let value = (quantized(&data, row * stored + col, bits)
                - quantized(&z, row * zero_stride + group, bits))
                * s[row * groups + group];
            let index = if recipe.kind == "matmul" {
                col * rows + row
            } else {
                row * cols + col
            };
            out[index] = value;
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../../../tests/unit/coreml_weights.rs"]
mod tests;
