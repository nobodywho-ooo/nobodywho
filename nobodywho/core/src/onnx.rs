//! Shared ONNX Runtime helpers used by both the TTS and STT modules.
//!
//! Exposes a [`Device`] enum for hardware-target selection and thin wrappers
//! around [`ort`] session construction so each backend doesn't repeat the
//! boilerplate.

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
pub fn execution_providers(device: Device) -> Vec<ort::ep::ExecutionProviderDispatch> {
    match device {
        Device::Cuda => vec![
            ort::ep::CUDA::default().build().error_on_failure(),
            ort::ep::CPU::default().build(),
        ],
        Device::Cpu => vec![ort::ep::CPU::default().build()],
        Device::Auto => vec![
            ort::ep::CUDA::default().build().fail_silently(),
            ort::ep::CPU::default().build(),
        ],
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
