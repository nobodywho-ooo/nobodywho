//! Shared ONNX Runtime helpers used by ONNX-backed modules.
//!
//! Exposes a [`Device`] enum for hardware-target selection and thin wrappers
//! around [`ort`] session construction so each backend doesn't repeat the
//! boilerplate.

use ort::environment::Environment;
#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
use ort::ep::{ExecutionProvider, CUDA};
use ort::ep::{ExecutionProviderDispatch, CPU};
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use std::path::Path;
use std::sync::Mutex;
use tracing::{info, warn};

const MPS_EXECUTION_PROVIDER: &str = "MLXExecutionProvider";
static MPS_REGISTRATION_LOCK: Mutex<()> = Mutex::new(());

/// Hardware target for ONNX Runtime execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    /// Try CUDA first, silently fall back to CPU if unavailable.
    Auto,
    Cpu,
    Cuda,
    /// Use the MLX execution provider on Apple Silicon, falling back to CPU.
    Mps,
}

/// Register the MLX execution-provider plugin used by [`Device::Mps`].
///
/// Call this once with the path to `libonnxruntime_mlx_ep.dylib` before loading
/// a session. Repeated calls are safe. If it is not registered, MPS requests
/// fall back to CPU.
pub fn register_mps_execution_provider(path: &Path) -> Result<(), ort::Error> {
    let _guard = MPS_REGISTRATION_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let environment = Environment::current()?;
    if environment
        .devices()
        .any(|device| matches!(device.ep(), Ok(MPS_EXECUTION_PROVIDER)))
    {
        return Ok(());
    }

    environment.register_ep_library(MPS_EXECUTION_PROVIDER, path)?;
    info!(path = %path.display(), "Registered MLX execution provider");
    Ok(())
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
fn cuda_execution_providers() -> Vec<ExecutionProviderDispatch> {
    vec![
        CUDA::default().build().error_on_failure(),
        CPU::default().build(),
    ]
}

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
)))]
fn cuda_execution_providers() -> Vec<ExecutionProviderDispatch> {
    warn!("CUDA is unavailable on this platform; falling back to CPU");
    vec![CPU::default().build()]
}

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
))]
fn auto_execution_providers() -> Vec<ExecutionProviderDispatch> {
    CUDA::default()
        .is_available()
        .unwrap_or(false)
        .then(|| CUDA::default().build().fail_silently())
        .into_iter()
        .chain(std::iter::once(CPU::default().build()))
        .collect()
}

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    any(target_arch = "x86", target_arch = "x86_64")
)))]
fn auto_execution_providers() -> Vec<ExecutionProviderDispatch> {
    vec![CPU::default().build()]
}

/// Build the static execution-provider list for a given [`Device`].
///
/// CPU is always appended alongside a GPU provider as a per-op fallback. MPS
/// is attached dynamically by [`load_session`] after its plugin is registered.
pub fn execution_providers(device: Device) -> Vec<ExecutionProviderDispatch> {
    match device {
        Device::Auto => auto_execution_providers(),
        Device::Cuda => cuda_execution_providers(),
        Device::Cpu | Device::Mps => vec![CPU::default().build()],
    }
}

fn with_mps_if_available(builder: SessionBuilder) -> Result<SessionBuilder, ort::Error> {
    let environment = Environment::current()?;
    let device = environment
        .devices()
        .find(|device| matches!(device.ep(), Ok(MPS_EXECUTION_PROVIDER)));

    match device {
        Some(device) => {
            info!("Using MLX execution provider");
            Ok(builder.with_devices([device], None)?)
        }
        None => {
            warn!("MPS is unavailable; falling back to CPU");
            Ok(builder.with_execution_providers([CPU::default().build()])?)
        }
    }
}

/// Open an ONNX model file and return a ready-to-run [`Session`].
pub fn load_session(path: &Path, device: Device) -> Result<Session, ort::Error> {
    #[cfg(not(all(
        any(target_os = "linux", target_os = "windows"),
        any(target_arch = "x86", target_arch = "x86_64")
    )))]
    if device == Device::Cuda {
        return Err(ort::Error::new("CUDA is unavailable on this platform"));
    }

    let builder = SessionBuilder::new()?.with_log_level(ort::logging::LogLevel::Warning)?;
    let mut builder = if device == Device::Mps {
        with_mps_if_available(builder)?
    } else {
        builder.with_execution_providers(execution_providers(device))?
    };
    builder.commit_from_file(path)
}
