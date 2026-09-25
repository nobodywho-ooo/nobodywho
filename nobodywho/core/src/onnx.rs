//! Shared ONNX Runtime helpers used by both the TextToSpeech and SpeechToText modules.
//!
//! Exposes a [`Device`] enum for hardware-target selection and thin wrappers
//! around [`ort`] session construction so each backend doesn't repeat the
//! boilerplate.

#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    target_arch = "x86_64"
))]
use ort::ep::{ExecutionProvider, CUDA};
use ort::ep::{ExecutionProviderDispatch, CPU};
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};
use ort::session::Session;
use std::path::Path;

/// Hardware target for ONNX Runtime execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Device {
    /// Try CUDA first, silently fall back to CPU if unavailable.
    Auto,
    Cpu,
    Cuda,
}

/// Build the execution-provider list for a given [`Device`].
///
/// CPU is always appended alongside CUDA as a per-op fallback — some ops lack
/// CUDA kernels, so CUDA still handles what it supports while CPU covers the rest.
pub fn execution_providers(device: Device) -> Vec<ExecutionProviderDispatch> {
    let mut eps: Vec<ExecutionProviderDispatch> = cuda_provider(device).into_iter().collect();
    eps.push(CPU::default().build());
    eps
}

// Same cfg as ort's `cuda` feature in core/Cargo.toml.
#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    target_arch = "x86_64"
))]
fn cuda_provider(device: Device) -> Option<ExecutionProviderDispatch> {
    match device {
        Device::Cuda => Some(CUDA::default().build().error_on_failure()),
        Device::Auto if CUDA::default().is_available().unwrap_or(false) => {
            Some(CUDA::default().build().fail_silently())
        }
        _ => None,
    }
}

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    target_arch = "x86_64"
)))]
fn cuda_provider(_device: Device) -> Option<ExecutionProviderDispatch> {
    None
}

/// Open an ONNX model file and return a ready-to-run [`Session`].
///
/// Returns `ort::Error` directly so callers can map it into their own domain
/// error type (`TextToSpeechError::Ort`, `SpeechToTextError::Ort`, …) using `?` plus a
/// `From<ort::Error>` impl.
pub fn load_session(path: &Path, device: Device) -> Result<Session, ort::Error> {
    load_session_with_optimization(path, device, GraphOptimizationLevel::All)
}

/// Like [`load_session`], but with an explicit graph optimization level.
pub fn load_session_with_optimization(
    path: &Path,
    device: Device,
    level: GraphOptimizationLevel,
) -> Result<Session, ort::Error> {
    #[cfg(not(all(
        any(target_os = "linux", target_os = "windows"),
        target_arch = "x86_64"
    )))]
    if device == Device::Cuda {
        return Err(ort::Error::new("CUDA is not supported on this platform"));
    }
    SessionBuilder::new()?
        .with_log_level(ort::logging::LogLevel::Warning)?
        .with_optimization_level(level)?
        .with_execution_providers(execution_providers(device))?
        .commit_from_file(path)
}
