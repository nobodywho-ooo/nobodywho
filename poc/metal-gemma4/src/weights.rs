use std::collections::HashMap;
use std::ffi::{CStr, CString, c_void};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::ptr;

use anyhow::{Context, anyhow, bail};
use half::f16;
use metal::{Buffer, DeviceRef, MTLResourceOptions};
use rayon::prelude::*;

unsafe extern "C" {
    fn dequantize_row_q4_K(x: *const c_void, y: *mut f32, k: i64);
    fn dequantize_row_q5_K(x: *const c_void, y: *mut f32, k: i64);
    fn dequantize_row_q6_K(x: *const c_void, y: *mut f32, k: i64);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WeightKind {
    F16,
    Q4K,
    Q5K,
    Q6K,
}

pub struct Weight {
    pub buffer: Buffer,
    pub embedding_buffer: Option<Buffer>,
    pub dimensions: [usize; 4],
    pub elements: usize,
    pub kind: WeightKind,
}

pub struct Weights {
    tensors: HashMap<String, Weight>,
    scalars: HashMap<String, f32>,
}

struct GgufGuard(*mut ggml_sys::gguf_context);
struct GgmlGuard(*mut ggml_sys::ggml_context);

impl Drop for GgmlGuard {
    fn drop(&mut self) {
        unsafe { ggml_sys::ggml_free(self.0) };
    }
}

impl Drop for GgufGuard {
    fn drop(&mut self) {
        unsafe { ggml_sys::gguf_free(self.0) };
    }
}

impl Weights {
    pub fn load(device: &DeviceRef, path: &Path) -> anyhow::Result<Self> {
        let path_string = CString::new(path.to_string_lossy().as_bytes())?;
        let mut source_context = ptr::null_mut();
        let context = unsafe {
            ggml_sys::gguf_init_from_file(
                path_string.as_ptr(),
                ggml_sys::gguf_init_params {
                    no_alloc: true,
                    ctx: &mut source_context,
                },
            )
        };
        if context.is_null() {
            bail!("failed to load GGUF model {}", path.display());
        }
        let _guard = GgufGuard(context);
        let _source_guard = (!source_context.is_null()).then(|| GgmlGuard(source_context));
        let count = unsafe { ggml_sys::gguf_get_n_tensors(context) };
        if count < 0 {
            bail!("GGUF has an invalid tensor count");
        }
        let data_offset = unsafe { ggml_sys::gguf_get_data_offset(context) } as u64;
        let file_size = std::fs::metadata(path)?.len();
        let mut file = File::open(path)?;
        let mut tensors = HashMap::with_capacity(count as usize);
        let mut scalars = HashMap::new();

        for index in 0..count as usize {
            if index % 25 == 0 {
                eprintln!("Loading tensor {index}/{count}");
            }
            let name_ptr = unsafe { ggml_sys::gguf_get_tensor_name(context, index as i64) };
            if name_ptr.is_null() {
                bail!("GGUF tensor {index} has no name");
            }
            let name = unsafe { CStr::from_ptr(name_ptr) }.to_str()?.to_owned();
            let kind = unsafe { ggml_sys::gguf_get_tensor_type(context, index as i64) };
            let ne_ptr = unsafe { ggml_sys::gguf_get_tensor_ne(context, index as i64) };
            if ne_ptr.is_null() {
                bail!("GGUF tensor {name} has no dimensions");
            }
            let mut dimensions = [1usize; 4];
            for axis in 0..4 {
                let value = unsafe { *ne_ptr.add(axis) };
                if value <= 0 {
                    bail!("GGUF tensor {name} has invalid shape");
                }
                dimensions[axis] = usize::try_from(value).context("tensor dimension overflow")?;
            }
            let elements = dimensions
                .into_iter()
                .try_fold(1usize, |total, n| total.checked_mul(n))
                .context("tensor element count overflow")?;
            let bytes = unsafe { ggml_sys::gguf_get_tensor_size(context, index as i64) };
            let start = data_offset
                .checked_add(
                    unsafe { ggml_sys::gguf_get_tensor_offset(context, index as i64) } as u64,
                )
                .context("tensor offset overflow")?;
            let end = start
                .checked_add(bytes as u64)
                .context("tensor range overflow")?;
            if end > file_size {
                bail!("GGUF tensor {name} exceeds the model file");
            }
            file.seek(SeekFrom::Start(start))?;
            let mut source = vec![0u8; bytes];
            file.read_exact(&mut source)?;
            if kind == ggml_sys::GGML_TYPE_F32 && elements == 1 {
                scalars.insert(name.clone(), f32::from_le_bytes(source[..4].try_into()?));
            }
            let weight_kind = weight_kind(kind, &name)?;
            let embedding_values = if weight_kind != WeightKind::F16
                && matches!(
                    name.as_str(),
                    "token_embd.weight" | "per_layer_token_embd.weight"
                ) {
                Some(decode(kind, &source, dimensions, elements, &name)?)
            } else {
                None
            };
            let converted_values = if weight_kind == WeightKind::F16 {
                Some(decode(kind, &source, dimensions, elements, &name)?)
            } else {
                None
            };
            let buffer = if let Some(values) = &converted_values {
                device.new_buffer_with_data(
                    values.as_ptr().cast(),
                    (values.len() * 2) as u64,
                    MTLResourceOptions::StorageModeShared,
                )
            } else {
                device.new_buffer_with_data(
                    source.as_ptr().cast(),
                    source.len() as u64,
                    MTLResourceOptions::StorageModeShared,
                )
            };
            let embedding_buffer = embedding_values.as_ref().map(|values| {
                device.new_buffer_with_data(
                    values.as_ptr().cast(),
                    (values.len() * 2) as u64,
                    MTLResourceOptions::StorageModeShared,
                )
            });
            tensors.insert(
                name,
                Weight {
                    buffer,
                    embedding_buffer,
                    dimensions,
                    elements,
                    kind: weight_kind,
                },
            );
        }
        Ok(Self { tensors, scalars })
    }

    pub fn get(&self, name: &str) -> anyhow::Result<&Weight> {
        self.tensors
            .get(name)
            .ok_or_else(|| anyhow!("model is missing tensor {name}"))
    }

    pub fn scalar(&self, name: &str) -> anyhow::Result<f32> {
        self.scalars
            .get(name)
            .copied()
            .ok_or_else(|| anyhow!("model is missing F32 scalar {name}"))
    }

    pub fn len(&self) -> usize {
        self.tensors.len()
    }
}

fn weight_kind(kind: ggml_sys::ggml_type, name: &str) -> anyhow::Result<WeightKind> {
    match kind {
        value
            if matches!(
                value,
                ggml_sys::GGML_TYPE_F32 | ggml_sys::GGML_TYPE_F16 | ggml_sys::GGML_TYPE_BF16
            ) =>
        {
            Ok(WeightKind::F16)
        }
        value if value == ggml_sys::GGML_TYPE_Q4_K => Ok(WeightKind::Q4K),
        value if value == ggml_sys::GGML_TYPE_Q5_K => Ok(WeightKind::Q5K),
        value if value == ggml_sys::GGML_TYPE_Q6_K => Ok(WeightKind::Q6K),
        _ => bail!("unsupported GGUF tensor type {kind:?} for {name}"),
    }
}

fn decode(
    kind: ggml_sys::ggml_type,
    source: &[u8],
    dimensions: [usize; 4],
    elements: usize,
    name: &str,
) -> anyhow::Result<Vec<u16>> {
    let rows = elements / dimensions[0];
    let mut output = vec![0u16; elements];
    match kind {
        k if k == ggml_sys::GGML_TYPE_F32 => {
            if source.len() != elements * 4 {
                bail!("GGUF tensor {name} has an invalid F32 size");
            }
            for (index, chunk) in source.chunks_exact(4).enumerate() {
                output[index] = f16::from_f32(f32::from_le_bytes(chunk.try_into()?)).to_bits();
            }
        }
        k if k == ggml_sys::GGML_TYPE_F16 => {
            if source.len() != elements * 2 {
                bail!("GGUF tensor {name} has an invalid F16 size");
            }
            for (index, chunk) in source.chunks_exact(2).enumerate() {
                output[index] = u16::from_le_bytes(chunk.try_into()?);
            }
        }
        k if k == ggml_sys::GGML_TYPE_BF16 => {
            if source.len() != elements * 2 {
                bail!("GGUF tensor {name} has an invalid BF16 size");
            }
            for (index, chunk) in source.chunks_exact(2).enumerate() {
                output[index] = f16::from_f32(f32::from_bits(
                    u32::from(u16::from_le_bytes(chunk.try_into()?)) << 16,
                ))
                .to_bits();
            }
        }
        k if [
            ggml_sys::GGML_TYPE_Q4_K,
            ggml_sys::GGML_TYPE_Q5_K,
            ggml_sys::GGML_TYPE_Q6_K,
        ]
        .contains(&k) =>
        {
            if dimensions[0] % 256 != 0 {
                bail!("quantized tensor {name} row width is not a multiple of 256");
            }
            let block_bytes = match k {
                x if x == ggml_sys::GGML_TYPE_Q4_K => 144,
                x if x == ggml_sys::GGML_TYPE_Q5_K => 176,
                _ => 210,
            };
            if source.len() != rows * (dimensions[0] / 256) * block_bytes {
                bail!("quantized tensor {name} has an invalid size");
            }
            let row_bytes = source.len() / rows;
            output
                .par_chunks_mut(dimensions[0])
                .zip(source.par_chunks(row_bytes))
                .for_each_init(
                    || vec![0f32; dimensions[0]],
                    |floats, (output, input)| {
                        unsafe {
                            match k {
                                x if x == ggml_sys::GGML_TYPE_Q4_K => dequantize_row_q4_K(
                                    input.as_ptr().cast(),
                                    floats.as_mut_ptr(),
                                    dimensions[0] as i64,
                                ),
                                x if x == ggml_sys::GGML_TYPE_Q5_K => dequantize_row_q5_K(
                                    input.as_ptr().cast(),
                                    floats.as_mut_ptr(),
                                    dimensions[0] as i64,
                                ),
                                _ => dequantize_row_q6_K(
                                    input.as_ptr().cast(),
                                    floats.as_mut_ptr(),
                                    dimensions[0] as i64,
                                ),
                            }
                        }
                        for (destination, value) in output.iter_mut().zip(floats.iter()) {
                            *destination = f16::from_f32(*value).to_bits();
                        }
                    },
                );
        }
        _ => bail!("unsupported GGUF tensor type {kind:?} for {name}"),
    }
    Ok(output)
}
