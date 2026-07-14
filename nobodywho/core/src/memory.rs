use crate::errors::{MemoryDetectionError, MemoryError};
use crate::host_memory::{self, HostMemory};
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

pub(crate) struct AvailableMemory {
    pub free_bytes: u64,
    pub usable_bytes: u64,
}

#[derive(Clone, Copy)]
struct GpuMemory {
    free_bytes: u64,
    total_bytes: u64,
    shares_host_memory: bool,
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

fn gpu_shares_host_memory(
    device_type: llama_cpp_2::LlamaBackendDeviceType,
    backend: &str,
    apple_unified_memory: bool,
) -> bool {
    matches!(
        device_type,
        llama_cpp_2::LlamaBackendDeviceType::IntegratedGpu
    ) || (apple_unified_memory && backend.eq_ignore_ascii_case("metal"))
}

fn available_model_memory_from(
    host: HostMemory,
    gpus: &[GpuMemory],
    use_gpu: bool,
) -> AvailableMemory {
    let dedicated_gpu = if use_gpu {
        gpus.iter()
            .filter(|gpu| !gpu.shares_host_memory)
            .max_by_key(|gpu| gpu.free_bytes)
            .copied()
    } else {
        None
    };
    let total_bytes = host
        .total_bytes
        .saturating_add(dedicated_gpu.map(|gpu| gpu.total_bytes).unwrap_or(0));
    let free_bytes = host
        .available_bytes
        .saturating_add(dedicated_gpu.map(|gpu| gpu.free_bytes).unwrap_or(0))
        .min(total_bytes);

    AvailableMemory {
        free_bytes,
        usable_bytes: free_bytes.min(total_bytes.saturating_mul(3) / 4),
    }
}

pub(crate) fn available_model_memory(
    use_gpu: bool,
) -> Result<AvailableMemory, MemoryDetectionError> {
    let host = host_memory::available()?;
    let gpus = llama_cpp_2::list_llama_ggml_backend_devices()
        .into_iter()
        .filter(|device| {
            matches!(
                device.device_type,
                llama_cpp_2::LlamaBackendDeviceType::Gpu
                    | llama_cpp_2::LlamaBackendDeviceType::IntegratedGpu
            )
        })
        .map(|device| GpuMemory {
            free_bytes: device_free(&device),
            total_bytes: device.memory_total as u64,
            shares_host_memory: gpu_shares_host_memory(
                device.device_type,
                &device.backend,
                cfg!(all(target_vendor = "apple", target_arch = "aarch64")),
            ),
        })
        .collect::<Vec<_>>();
    Ok(available_model_memory_from(host, &gpus, use_gpu))
}

fn read_gguf_model_info(path: &Path) -> Option<GgufModelInfo> {
    let ctx = llama_cpp_2::gguf::GgufContext::from_file(path)?;
    let file_size = std::fs::metadata(path).ok()?.len();

    let find_u32 = |suffix: &str| {
        (0..ctx.n_kv())
            .find(|&i| ctx.key_at(i).is_some_and(|k| k.ends_with(suffix)))
            .map(|i| ctx.val_u32(i))
    };

    // llama.cpp counts offloadable layers as block_count + 1. The extra layer
    // is the output/embedding layer. If we only offload block_count layers,
    // layer 0 stays on CPU and every token requires a CPU<->GPU round-trip,
    // which can degrade performance by 3-30x depending on model size.
    let block_count = find_u32(".block_count")?;
    Some(GgufModelInfo {
        n_layers: block_count + 1,
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

#[cfg(test)]
mod tests {
    use super::*;
    use llama_cpp_2::LlamaBackendDeviceType::{Gpu, IntegratedGpu};

    const GIB: u64 = 1024 * 1024 * 1024;

    fn host(available: u64, total: u64) -> HostMemory {
        HostMemory {
            available_bytes: available,
            total_bytes: total,
        }
    }

    fn gpu(free: u64, total: u64, shares_host_memory: bool) -> GpuMemory {
        GpuMemory {
            free_bytes: free,
            total_bytes: total,
            shares_host_memory,
        }
    }

    #[test]
    fn reserves_a_quarter_of_total_memory() {
        let memory = available_model_memory_from(host(15 * GIB, 16 * GIB), &[], false);
        assert_eq!(memory.free_bytes, 15 * GIB);
        assert_eq!(memory.usable_bytes, 12 * GIB);
    }

    #[test]
    fn uses_constrained_available_host_memory() {
        let memory = available_model_memory_from(host(3 * GIB, 16 * GIB), &[], false);
        assert_eq!(memory.free_bytes, 3 * GIB);
        assert_eq!(memory.usable_bytes, 3 * GIB);
    }

    #[test]
    fn does_not_double_count_integrated_gpu_memory() {
        let memory = available_model_memory_from(
            host(6 * GIB, 8 * GIB),
            &[gpu(6 * GIB, 8 * GIB, true)],
            true,
        );
        assert_eq!(memory.free_bytes, 6 * GIB);
    }

    #[test]
    fn treats_apple_metal_gpu_as_unified_memory() {
        assert!(gpu_shares_host_memory(Gpu, "Metal", true));
        let memory = available_model_memory_from(
            host(6 * GIB, 8 * GIB),
            &[gpu(6 * GIB, 8 * GIB, true)],
            true,
        );
        assert_eq!(memory.free_bytes, 6 * GIB);
    }

    #[test]
    fn does_not_treat_discrete_metal_gpu_as_unified_memory() {
        assert!(!gpu_shares_host_memory(Gpu, "Metal", false));
        assert!(gpu_shares_host_memory(IntegratedGpu, "Vulkan", false));
    }

    #[test]
    fn adds_discrete_gpu_memory_when_enabled() {
        let gpus = [gpu(7 * GIB, 8 * GIB, false)];
        let with_gpu = available_model_memory_from(host(6 * GIB, 8 * GIB), &gpus, true);
        let without_gpu = available_model_memory_from(host(6 * GIB, 8 * GIB), &gpus, false);
        assert_eq!(with_gpu.free_bytes, 13 * GIB);
        assert_eq!(without_gpu.free_bytes, 6 * GIB);
    }
}
