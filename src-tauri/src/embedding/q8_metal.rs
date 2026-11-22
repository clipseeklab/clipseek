//! Expand verified blockwise Q8 weights once per session for efficient Metal MatMul.
//! The quantized values are preserved; this increases resident memory to float32 weights.
use anyhow::{bail, Context, Result};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

#[derive(Clone)]
struct Field {
    tag: u64,
    wire: u8,
    data: Vec<u8>,
}
fn varint(input: &[u8], pos: &mut usize) -> Result<u64> {
    let mut value = 0;
    for shift in (0..70).step_by(7) {
        let byte = *input.get(*pos).context("Truncated ONNX varint")?;
        *pos += 1;
        if shift == 63 && byte > 1 {
            bail!("Invalid ONNX varint");
        }
        value |= ((byte & 127) as u64) << shift;
        if byte < 128 {
            return Ok(value);
        }
    }
    bail!("Invalid ONNX varint")
}
fn vint(mut value: u64) -> Vec<u8> {
    let mut out = Vec::new();
    while value >= 128 {
        out.push(value as u8 | 128);
        value >>= 7;
    }
    out.push(value as u8);
    out
}
fn parse(input: &[u8]) -> Result<Vec<Field>> {
    let mut pos = 0;
    let mut out = Vec::new();
    while pos < input.len() {
        let key = varint(input, &mut pos)?;
        let wire = (key & 7) as u8;
        let data = match wire {
            0 => {
                let start = pos;
                varint(input, &mut pos)?;
                input[start..pos].to_vec()
            }
            1 | 2 | 5 => {
                let len = match wire {
                    1 => 8,
                    5 => 4,
                    _ => usize::try_from(varint(input, &mut pos)?)?,
                };
                let end = pos.checked_add(len).context("ONNX field overflow")?;
                let data = input
                    .get(pos..end)
                    .context("Truncated ONNX field")?
                    .to_vec();
                pos = end;
                data
            }
            _ => bail!("Unsupported ONNX wire type"),
        };
        out.push(Field {
            tag: key >> 3,
            wire,
            data,
        });
    }
    Ok(out)
}
fn encode(fields: &[Field]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in fields {
        out.extend(vint(f.tag << 3 | f.wire as u64));
        if f.wire == 2 {
            out.extend(vint(f.data.len() as u64));
        }
        out.extend(&f.data);
    }
    out
}
fn bytes(tag: u64, data: impl AsRef<[u8]>) -> Field {
    Field {
        tag,
        wire: 2,
        data: data.as_ref().into(),
    }
}
fn integer(tag: u64, value: u64) -> Field {
    Field {
        tag,
        wire: 0,
        data: vint(value),
    }
}
fn text(fields: &[Field], tag: u64) -> Result<String> {
    Ok(String::from_utf8(
        fields
            .iter()
            .find(|f| f.tag == tag)
            .context("Missing ONNX field")?
            .data
            .clone(),
    )?)
}
fn number(fields: &[Field], tag: u64) -> Result<u64> {
    varint(
        &fields
            .iter()
            .find(|f| f.tag == tag)
            .context("Missing ONNX integer")?
            .data,
        &mut 0,
    )
}
fn raw(tensor: &[Field], root: &Path) -> Result<Vec<u8>> {
    if let Some(f) = tensor.iter().find(|f| f.tag == 9) {
        return Ok(f.data.clone());
    }
    let mut external = HashMap::new();
    for f in tensor.iter().filter(|f| f.tag == 13) {
        let kv = parse(&f.data)?;
        external.insert(text(&kv, 1)?, text(&kv, 2)?);
    }
    let location = external
        .get("location")
        .context("Missing Q8 external data")?;
    if Path::new(location).components().count() != 1 {
        bail!("Unsafe ONNX external data path");
    }
    let offset: u64 = external
        .get("offset")
        .context("Missing external offset")?
        .parse()?;
    let length: usize = external
        .get("length")
        .context("Missing external length")?
        .parse()?;
    let mut file = fs::File::open(root.join(location))?;
    if offset
        .checked_add(length as u64)
        .context("External data overflow")?
        > file.metadata()?.len()
    {
        bail!("Invalid external data range");
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut data = vec![0; length];
    file.read_exact(&mut data)?;
    Ok(data)
}
fn external_tensor(name: &str, k: usize, n: usize, offset: u64) -> Field {
    let mut fields = vec![
        integer(1, k as u64),
        integer(1, n as u64),
        integer(2, 1),
        bytes(8, name),
        integer(14, 1),
    ];
    for (key, value) in [
        ("location", "expanded.bin".into()),
        ("offset", offset.to_string()),
        ("length", (k * n * 4).to_string()),
    ] {
        fields.push(bytes(13, encode(&[bytes(1, key), bytes(2, value)])));
    }
    bytes(5, encode(&fields))
}

pub struct ExpandedModel {
    pub path: PathBuf,
    _directory: tempfile::TempDir,
}
/// Called only after the original model catalogue passed integrity verification.
pub fn expand(path: &Path) -> Result<ExpandedModel> {
    let root = path.parent().context("Missing model directory")?;
    let mut model = parse(&fs::read(path)?)?;
    let graph = model
        .iter_mut()
        .find(|f| f.tag == 7)
        .context("Missing ONNX graph")?;
    let mut fields = parse(&graph.data)?;
    let tensors: HashMap<String, Vec<Field>> = fields
        .iter()
        .filter(|f| f.tag == 5)
        .map(|f| {
            let t = parse(&f.data)?;
            Ok((text(&t, 8)?, t))
        })
        .collect::<Result<_>>()?;
    let directory = tempfile::tempdir()?;
    let mut weights = fs::File::create(directory.path().join("expanded.bin"))?;
    let mut offset = 0;
    let mut additions = Vec::new();
    let mut replaced = HashSet::new();
    for field in fields.iter_mut().filter(|f| f.tag == 1) {
        let mut node = parse(&field.data)?;
        if text(&node, 4)? != "MatMulNBits" {
            continue;
        }
        let mut attrs = HashMap::new();
        for f in node.iter().filter(|f| f.tag == 5) {
            let a = parse(&f.data)?;
            attrs.insert(text(&a, 1)?, number(&a, 3)? as usize);
        }
        let get = |key: &str| {
            attrs
                .get(key)
                .copied()
                .with_context(|| format!("Missing Q8 attribute {key}"))
        };
        let (k, n, block) = (get("K")?, get("N")?, get("block_size")?);
        if get("bits")? != 8 || block == 0 || k == 0 || n == 0 {
            bail!("Unsupported Q8 matrix layout");
        }
        let inputs: Vec<String> = node
            .iter()
            .filter(|f| f.tag == 1)
            .map(|f| String::from_utf8(f.data.clone()))
            .collect::<std::result::Result<_, _>>()?;
        if inputs.len() != 4 {
            bail!("Unsupported Q8 matrix inputs");
        }
        let data: Vec<Vec<u8>> = inputs[1..]
            .iter()
            .map(|name| raw(tensors.get(name).context("Missing Q8 initializer")?, root))
            .collect::<Result<_>>()?;
        let blocks = k.div_ceil(block);
        if data[0].len() != n * blocks * block
            || data[1].len() != n * blocks * 4
            || data[2].len() != n * blocks
        {
            bail!("Q8 tensor size mismatch");
        }
        let mut expanded = vec![0u8; k * n * 4];
        for col in 0..n {
            for row in 0..k {
                let group = col * blocks + row / block;
                let scale = f32::from_le_bytes(data[1][group * 4..group * 4 + 4].try_into()?);
                let value =
                    (data[0][col * blocks * block + row] as f32 - data[2][group] as f32) * scale;
                expanded[(row * n + col) * 4..(row * n + col + 1) * 4]
                    .copy_from_slice(&value.to_le_bytes());
            }
        }
        weights.write_all(&expanded)?;
        let name = format!("{}_metal_q8", text(&node, 3)?);
        additions.push(external_tensor(&name, k, n, offset));
        offset += expanded.len() as u64;
        replaced.extend(inputs[1..].iter().cloned());
        node.retain(|f| ![1, 4, 5, 7].contains(&f.tag));
        node.extend([bytes(1, &inputs[0]), bytes(1, name), bytes(4, "MatMul")]);
        field.data = encode(&node);
    }
    if additions.is_empty() {
        bail!("No Q8 matrices found");
    }
    let used: HashSet<String> = fields
        .iter()
        .filter(|f| f.tag == 1)
        .map(|f| parse(&f.data))
        .collect::<Result<Vec<_>>>()?
        .iter()
        .flat_map(|n| {
            n.iter()
                .filter(|f| f.tag == 1)
                .filter_map(|f| String::from_utf8(f.data.clone()).ok())
        })
        .collect();
    let mut retained = Vec::new();
    for field in fields {
        if field.tag == 5 {
            let name = text(&parse(&field.data)?, 8)?;
            if replaced.contains(&name) && !used.contains(&name) {
                continue;
            }
        }
        retained.push(field);
    }
    let mut fields = retained;
    // Keep unchanged tensors pointing at their original, verified bytes without copying weights.
    for tensor in tensors.values() {
        for f in tensor.iter().filter(|f| f.tag == 13) {
            let kv = parse(&f.data)?;
            if text(&kv, 1)? == "location" {
                let name = text(&kv, 2)?;
                if Path::new(&name).components().count() != 1 {
                    bail!("Unsafe external tensor path");
                }
                let dest = directory.path().join(&name);
                if !dest.exists() {
                    fs::hard_link(root.join(&name), &dest)
                        .or_else(|_| fs::copy(root.join(&name), &dest).map(|_| ()))?;
                }
            }
        }
    }
    fields.extend(additions);
    graph.data = encode(&fields);
    let output = directory.path().join("model.onnx");
    fs::write(&output, encode(&model))?;
    Ok(ExpandedModel {
        path: output,
        _directory: directory,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/q8_metal.rs"]
mod tests;
