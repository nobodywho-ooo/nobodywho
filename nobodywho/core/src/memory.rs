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
