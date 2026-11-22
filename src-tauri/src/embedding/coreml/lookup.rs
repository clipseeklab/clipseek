//! Sparse CPU token lookup avoids sending the full vocabulary table to the GPU.
use super::weights::{source_path, Recipe, Span};
use anyhow::{bail, Context, Result};
use memmap2::{Mmap, MmapOptions};
use std::{collections::HashMap, fs::File, path::Path, sync::Arc};
struct Window {
    map: Arc<Mmap>,
    start: usize,
    length: usize,
}
impl Window {
    fn open(root: &Path, span: &Span, maps: &mut HashMap<String, Arc<Mmap>>) -> Result<Self> {
        let map = if let Some(map) = maps.get(&span.file) {
            Arc::clone(map)
        } else {
            let file = File::open(source_path(root, &span.file)?)?;
            // Verified model files are immutable while the shared encoder is resident.
            let map = Arc::new(unsafe { MmapOptions::new().map(&file)? });
            maps.insert(span.file.clone(), Arc::clone(&map));
            map
        };
        let start = usize::try_from(span.offset)?;
        if start
            .checked_add(span.bytes)
            .context("Token range overflow")?
            > map.len()
        {
            bail!("Invalid token table range");
        }
        Ok(Self {
            map,
            start,
            length: span.bytes,
        })
    }
    fn bytes(&self) -> &[u8] {
        &self.map[self.start..self.start + self.length]
    }
    fn float(&self, index: usize) -> f32 {
        let b = &self.bytes()[index * 4..index * 4 + 4];
        f32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
}
pub(super) struct Table {
    data: Window,
    scales: Option<Window>,
    zeros: Option<Window>,
    rows: usize,
    width: usize,
    bits: usize,
    block: usize,
    groups: usize,
    zero_stride: usize,
}
impl Table {
    pub fn open(root: &Path, recipe: &Recipe, width: usize) -> Result<Self> {
        let shape = &recipe.data.shape;
        if shape.len() != 2 || shape[0] == 0 || width != 512 {
            bail!("Invalid token table shape");
        }
        let rows = shape[0];
        let mut maps = HashMap::new();
        let data = Window::open(root, &recipe.data, &mut maps)?;
        if recipe.kind == "plain" {
            if recipe.data.dtype != "float32"
                || shape[1] != width
                || data.length
                    != rows
                        .checked_mul(width)
                        .and_then(|x| x.checked_mul(4))
                        .context("Token shape overflow")?
            {
                bail!("Invalid plain token table");
            }
            return Ok(Self {
                data,
                scales: None,
                zeros: None,
                rows,
                width,
                bits: 32,
                block: 0,
                groups: 0,
                zero_stride: 0,
            });
        }
        let bits = recipe.bits.context("Missing token quantization bits")?;
        let block = recipe.block.context("Missing token block")?;
        if recipe.kind != "gather"
            || ![4, 8].contains(&bits)
            || block == 0
            || recipe.data.dtype != "uint8"
            || shape[1] * 8 / bits != width
        {
            bail!("Unsupported token quantization");
        }
        let groups = width.div_ceil(block);
        let zero_stride = groups.div_ceil(8 / bits);
        let s = recipe.scales.as_ref().context("Missing token scales")?;
        let z = recipe.zeros.as_ref().context("Missing token zero points")?;
        let scales = Window::open(root, s, &mut maps)?;
        let zeros = Window::open(root, z, &mut maps)?;
        if s.dtype != "float32"
            || z.dtype != "uint8"
            || data.length != rows.checked_mul(shape[1]).context("Token data overflow")?
            || scales.length
                != rows
                    .checked_mul(groups)
                    .and_then(|x| x.checked_mul(4))
                    .context("Token scale overflow")?
            || zeros.length
                != rows
                    .checked_mul(zero_stride)
                    .context("Token zero-point overflow")?
        {
            bail!("Token quantization size mismatch");
        }
        Ok(Self {
            data,
            scales: Some(scales),
            zeros: Some(zeros),
            rows,
            width,
            bits,
            block,
            groups,
            zero_stride,
        })
    }
    pub fn row(&self, id: i64) -> Result<Vec<f32>> {
        let row = usize::try_from(id).context("Invalid token identifier")?;
        if row >= self.rows {
            bail!("Token identifier exceeds the vocabulary");
        }
        if self.bits == 32 {
            return Ok((0..self.width)
                .map(|c| self.data.float(row * self.width + c))
                .collect());
        }
        let scales = self
            .scales
            .as_ref()
            .context("Missing token scale mapping")?;
        let zeros = self.zeros.as_ref().context("Missing token zero mapping")?;
        let packed = self.width * self.bits / 8;
        Ok((0..self.width)
            .map(|col| {
                let group = col / self.block;
                let byte = self.data.bytes()[row * packed + col * self.bits / 8];
                let zero = zeros.bytes()[row * self.zero_stride + group * self.bits / 8];
                let (q, z) = if self.bits == 8 {
                    (byte, zero)
                } else {
                    (
                        (byte >> ((col % 2) * 4)) & 15,
                        (zero >> ((group % 2) * 4)) & 15,
                    )
                };
                (q as f32 - z as f32) * scales.float(row * self.groups + group)
            })
            .collect())
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/coreml_lookup.rs"]
mod tests;
