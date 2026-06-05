//! Shared ONNX Runtime helpers used by both the TTS and STT modules.
//!
//! Exposes a [`Device`] enum for hardware-target selection and thin wrappers
//! around [`ort`] session construction so each backend doesn't repeat the
//! boilerplate.

use ort::ep::{ExecutionProvider, ExecutionProviderDispatch, CPU, CUDA};
use ort::session::builder::SessionBuilder;
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
    match device {
        Device::Cuda => vec![
            CUDA::default().build().error_on_failure(),
            CPU::default().build(),
        ],
        Device::Cpu => vec![CPU::default().build()],
        Device::Auto => {
            let mut eps: Vec<ExecutionProviderDispatch> = Vec::new();
            if CUDA::default().is_available().unwrap_or(false) {
                eps.push(CUDA::default().build().fail_silently());
            }
            eps.push(CPU::default().build());
            eps
        }
    }
}

/// Open an ONNX model file and return a ready-to-run [`Session`].
///
/// Returns `ort::Error` directly so callers can map it into their own domain
/// error type (`TtsError::Ort`, `SttError::Ort`, …) using `?` plus a
/// `From<ort::Error>` impl.
pub fn load_session(path: &Path, device: Device) -> Result<Session, ort::Error> {
    SessionBuilder::new()?
        .with_log_level(ort::logging::LogLevel::Warning)?
        .with_execution_providers(execution_providers(device))?
        .commit_from_file(path)
}
