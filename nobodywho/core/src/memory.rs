use crate::errors::MemoryError;
use std::path::Path;
use tracing::warn;

pub(crate) struct GgufModelInfo {
    pub n_layers: u32,
    pub file_size: u64,
}

pub(crate) struct ModelArchitecture {
    pub n_layers: u32,
    pub n_embd: u32,
    pub n_head: u32,
    pub n_head_kv: u32,
}

pub struct LoadingPlan {
    pub gpu_layers: u32,
    pub warnings: Vec<String>,
}

fn device_free(d: &llama_cpp_2::LlamaBackendDevice) -> u64 {
    let memory_free = d.memory_free as u64;
    let memory_total = d.memory_total as u64;

    if memory_free > memory_total {
        warn!("Detected more free memory on the device than the total memory of the device. This should not happen.");
    }

    memory_free.min(memory_total)
}

fn select_best_gpu() -> Option<llama_cpp_2::LlamaBackendDevice> {
    llama_cpp_2::list_llama_ggml_backend_devices()
        .into_iter()
        .filter(|d| {
            matches!(
                d.device_type,
                llama_cpp_2::LlamaBackendDeviceType::Gpu
                    | llama_cpp_2::LlamaBackendDeviceType::IntegratedGpu
            )
        })
        .max_by_key(|d| {
            let is_gpu = matches!(d.device_type, llama_cpp_2::LlamaBackendDeviceType::Gpu);
            (is_gpu, device_free(d))
        })
}

fn read_gguf_model_info(path: &Path) -> Option<GgufModelInfo> {
    let ctx = llama_cpp_2::gguf::GgufContext::from_file(path)?;
    let file_size = std::fs::metadata(path).ok()?.len();

    let find_u32 = |suffix: &str| {
        (0..ctx.n_kv())
            .find(|&i| ctx.key_at(i).is_some_and(|k| k.ends_with(suffix)))
            .map(|i| ctx.val_u32(i))
    };

    Some(GgufModelInfo {
        n_layers: find_u32(".block_count")?,
        file_size,
    })
}

fn estimate_per_layer_bytes(info: &GgufModelInfo) -> u64 {
    if info.n_layers == 0 {
        return info.file_size;
    }
    info.file_size / info.n_layers as u64
}

fn estimate_kv_cache_bytes(arch: &ModelArchitecture, n_ctx: u32) -> u64 {
    // KV cache: 2 (K+V) * n_layers * n_ctx * n_head_kv * head_dim * 2 bytes (f16)
    let head_dim = arch.n_embd.checked_div(arch.n_head).unwrap_or(64) as u64;
    2 * arch.n_layers as u64 * n_ctx as u64 * arch.n_head_kv as u64 * head_dim * 2
}

/// Compute how many GPU layers to offload based on available VRAM.
/// This function never fails: on any estimation error it falls back to current behavior.
pub(crate) fn plan_model_loading(
    model_path: &Path,
    mmproj_path: Option<&Path>,
    use_gpu: bool,
) -> LoadingPlan {
    if !use_gpu {
        return LoadingPlan {
            gpu_layers: 0,
            warnings: vec![],
        };
    }

    let Some(info) = read_gguf_model_info(model_path) else {
        return LoadingPlan {
            gpu_layers: u32::MAX,
            warnings: vec![format!(
                "Could not parse GGUF metadata from {}. Falling back to full GPU offload.",
                model_path.display()
            )],
        };
    };

    let Some(gpu) = select_best_gpu() else {
        return LoadingPlan {
            gpu_layers: 0,
            warnings: vec![],
        };
    };

    let gpu_free = device_free(&gpu);
    let mut available = gpu_free;

    // Reserve space for projection model
    if let Some(mmproj) = mmproj_path {
        match std::fs::metadata(mmproj) {
            Ok(meta) => {
                let mmproj_size = meta.len();
                available = available.saturating_sub(mmproj_size);
            }
            Err(e) => {
                warn!(
                    "Could not read mmproj file size for {}: {}",
                    mmproj.display(),
                    e
                );
            }
        }
    }

    let per_layer = estimate_per_layer_bytes(&info);
    if per_layer == 0 {
        return LoadingPlan {
            gpu_layers: u32::MAX,
            warnings: vec![],
        };
    }

    let gpu_layers_estimate = (available / per_layer).min(info.n_layers as u64) as u32;
    let min_useful_layers = (info.n_layers as f64 * 0.1).ceil() as u32;

    let mut warnings = vec![];

    if gpu_layers_estimate < min_useful_layers {
        let available_gb = gpu_free as f64 / 1e9;
        let model_gb = info.file_size as f64 / 1e9;
        warnings.push(format!(
            "Only {gpu_layers_estimate}/{} layers would fit in GPU VRAM \
             ({available_gb:.1} GB free, model is {model_gb:.1} GB). \
             Skipping GPU offload and running on CPU only.",
            info.n_layers
        ));
        return LoadingPlan {
            gpu_layers: 0,
            warnings,
        };
    }

    if gpu_layers_estimate < info.n_layers {
        let available_gb = gpu_free as f64 / 1e9;
        warnings.push(format!(
            "Model does not fully fit in GPU VRAM ({available_gb:.1} GB free). \
             Offloading {gpu_layers_estimate}/{} layers to GPU; \
             remaining layers will run on CPU.",
            info.n_layers
        ));
    }

    LoadingPlan {
        gpu_layers: gpu_layers_estimate,
        warnings,
    }
}

// --- Dry-run memory estimation ---

struct GgufModelInfoExtended {
    n_layers: u32,
    file_size: u64,
    n_embd: u32,
    n_head: u32,
    n_head_kv: u32,
}

fn read_gguf_model_info_extended(path: &str) -> Option<GgufModelInfoExtended> {
    let ctx = llama_cpp_2::gguf::GgufContext::from_file(Path::new(path))?;
    let file_size = std::fs::metadata(path).ok()?.len();

    let find_u32 = |suffix: &str| {
        (0..ctx.n_kv())
            .find(|&i| ctx.key_at(i).is_some_and(|k| k.ends_with(suffix)))
            .map(|i| ctx.val_u32(i))
    };

    Some(GgufModelInfoExtended {
        n_layers: find_u32(".block_count").unwrap_or(0),
        file_size,
        n_embd: find_u32(".embedding_length").unwrap_or(0),
        n_head: find_u32(".attention.head_count").unwrap_or(0),
        n_head_kv: find_u32(".attention.head_count_kv").unwrap_or(0),
    })
}

/// Estimate memory requirements for a model without loading it.
/// Returns `(model_weights_bytes, kv_cache_bytes)`.
///
/// `kv_type_k_bytes` and `kv_type_v_bytes` are bytes per element for the KV cache types
/// (e.g. 4.0 = f32, 2.0 = f16, 1.0 = q8_0, 0.5 = q4_0).
/// File size is used as a proxy for model weight memory; llama.cpp runtime overhead is not included.
pub fn dry_run_memory_estimate(
    model_path: &str,
    n_ctx: u32,
    kv_type_k_bytes: f32,
    kv_type_v_bytes: f32,
) -> Result<(u64, u64), MemoryError> {
    let info = read_gguf_model_info_extended(model_path)
        .ok_or_else(|| MemoryError::GgufReadError(model_path.to_string()))?;

    let head_dim = if info.n_head > 0 {
        info.n_embd / info.n_head
    } else {
        64
    } as f64;
    let tokens = info.n_layers as f64 * n_ctx as f64 * info.n_head_kv as f64 * head_dim;
    let kv_bytes = (tokens * kv_type_k_bytes as f64 + tokens * kv_type_v_bytes as f64) as u64;

    Ok((info.file_size, kv_bytes))
}

/// Compute the largest context size that fits within a fraction of available memory, up to a maximum.
///
/// When a GPU is present, uses GPU VRAM by default. Set `include_cpu` to also count CPU RAM —
/// useful if you don't mind the KV cache spilling to CPU, but note this is significantly slower.
/// Model weights (file size) are subtracted from the budget before computing KV cache headroom.
/// `kv_type_k_bytes` / `kv_type_v_bytes` are bytes per element (4.0=f32, 2.0=f16, 1.0=q8_0, 0.5=q4_0).
pub fn compute_context_size_for_budget(
    model_path: &str,
    memory_fraction: f64,
    max_n_ctx: u32,
    kv_type_k_bytes: f32,
    kv_type_v_bytes: f32,
    include_cpu: bool,
) -> Result<u32, MemoryError> {
    let info = read_gguf_model_info_extended(model_path)
        .ok_or_else(|| MemoryError::GgufReadError(model_path.to_string()))?;

    let devices = llama_cpp_2::list_llama_ggml_backend_devices();
    let cpu_free = devices
        .iter()
        .find(|d| matches!(d.device_type, llama_cpp_2::LlamaBackendDeviceType::Cpu))
        .map(|d| device_free(d))
        .unwrap_or(0);
    let total_available = match select_best_gpu() {
        Some(gpu) if include_cpu => device_free(&gpu) + cpu_free,
        Some(gpu) => device_free(&gpu),
        None => cpu_free,
    };

    let budget = (total_available as f64 * memory_fraction.clamp(0.0, 1.0)) as u64;
    let remaining = budget.saturating_sub(info.file_size);

    if remaining == 0 {
        return Err(MemoryError::InsufficientMemory {
            required_gb: info.file_size as f64 / 1e9,
            available_gb: budget as f64 / 1e9,
            suggestion: "increase memory_fraction or use a smaller model".to_string(),
        });
    }

    let head_dim = if info.n_head > 0 {
        info.n_embd / info.n_head
    } else {
        64
    } as f64;
    let bytes_per_token = info.n_layers as f64
        * info.n_head_kv as f64
        * head_dim
        * (kv_type_k_bytes + kv_type_v_bytes) as f64;

    if bytes_per_token == 0.0 {
        return Ok(max_n_ctx);
    }

    let n_ctx = ((remaining as f64 / bytes_per_token) as u32).min(max_n_ctx);

    if n_ctx == 0 {
        return Err(MemoryError::InsufficientMemory {
            required_gb: (info.file_size as f64 + bytes_per_token) / 1e9,
            available_gb: budget as f64 / 1e9,
            suggestion: "increase memory_fraction or use a smaller model".to_string(),
        });
    }

    Ok(n_ctx)
}

// --- Context planning ---

pub struct ContextPlan {
    pub n_ctx: u32,
    pub n_ubatch: u32,
    pub warnings: Vec<String>,
}

/// Plan context parameters and validate memory for the requested context size.
/// Computes n_ubatch, checks both CPU and GPU memory,
/// and returns warnings (e.g. multimodal context too small).
pub(crate) fn plan_context(
    n_ctx: u32,
    has_projection_model: bool,
    arch: ModelArchitecture,
) -> Result<ContextPlan, MemoryError> {
    let n_ubatch = if has_projection_model {
        n_ctx.min(2048)
    } else {
        n_ctx.min(512)
    };

    let mut warnings = vec![];
    if has_projection_model && n_ctx < 2048 {
        warnings.push(
            "Context size is less than 2048, which is the default minimum for ingesting images. \
             This can cause issues."
                .to_string(),
        );
    }

    let devices = llama_cpp_2::list_llama_ggml_backend_devices();
    let cpu_free: u64 = devices
        .iter()
        .find(|d| matches!(d.device_type, llama_cpp_2::LlamaBackendDeviceType::Cpu))
        .map(device_free)
        .unwrap_or(0);

    let (total_available, available_gb_label) = match select_best_gpu() {
        Some(gpu) => (device_free(&gpu) + cpu_free, device_free(&gpu) as f64 / 1e9),
        None => (cpu_free, cpu_free as f64 / 1e9),
    };

    let kv_estimate = estimate_kv_cache_bytes(&arch, n_ctx);

    if kv_estimate > total_available {
        let max_n_ctx = (total_available * n_ctx as u64 / kv_estimate) as u32;
        return Err(MemoryError::InsufficientMemory {
            required_gb: kv_estimate as f64 / 1e9,
            available_gb: available_gb_label,
            suggestion: format!("reduce n_ctx to {max_n_ctx}"),
        });
    }

    Ok(ContextPlan {
        n_ctx,
        n_ubatch,
        warnings,
    })
}
