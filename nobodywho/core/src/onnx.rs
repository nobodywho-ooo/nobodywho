//! Shared ONNX Runtime helpers used by both the TextToSpeech and SpeechToText modules.
//!
//! Exposes a [`Device`] enum for hardware-target selection and thin wrappers
//! around [`ort`] session construction so each backend doesn't repeat the
//! boilerplate.

use ort::environment::Environment;
use ort::ep::{ExecutionProviderDispatch, CPU};
use ort::session::Session;
use std::path::Path;
use std::sync::LazyLock;

/// The process-wide ONNX Runtime environment.
static ENVIRONMENT: LazyLock<Result<Environment, ort::Error>> = LazyLock::new(|| {
    #[cfg(target_os = "emscripten")]
    set_emscripten_ort_api()?;
    ort::init()
        .with_name("nobodywho")
        .with_telemetry(false)
        .build()
});

/// ort uses `alternative-backend` on Emscripten, so give it the ONNX Runtime that build.rs linked from `ORT_LIB_PATH`.
///
/// Without one, ort gets its stub API, since even creating an `ort::Error` panics with no API set.
#[cfg(target_os = "emscripten")]
fn set_emscripten_ort_api() -> Result<(), ort::Error> {
    #[cfg(nobodywho_static_ort)]
    {
        // SAFETY: build.rs linked ONNX Runtime, which defines `OrtGetApiBase`.
        let api = unsafe { ((*ort::sys::OrtGetApiBase()).GetApi)(ort::sys::ORT_API_VERSION) };
        // SAFETY: a non-null `GetApi` result points to a static `OrtApi`.
        if let Some(api) = unsafe { api.as_ref() } {
            ort::set_api(api.clone());
            return Ok(());
        }
        ort::set_api(ort::sys::stub::api());
        Err(ort::Error::new(
            "the linked ONNX Runtime doesn't support the API version ort needs",
        ))
    }
    #[cfg(not(nobodywho_static_ort))]
    {
        ort::set_api(ort::sys::stub::api());
        Err(ort::Error::new(
            "built without ONNX Runtime; build it for Emscripten and set ORT_LIB_PATH when building NobodyWho",
        ))
    }
}

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
///
/// Fails for [`Device::Cuda`] on platforms built without CUDA support.
pub fn execution_providers(device: Device) -> Result<Vec<ExecutionProviderDispatch>, ort::Error> {
    let mut eps: Vec<ExecutionProviderDispatch> = cuda_provider(device)?.into_iter().collect();
    eps.push(CPU::default().build());
    Ok(eps)
}

// Same cfg as ort's `cuda` feature in core/Cargo.toml.
#[cfg(all(
    any(target_os = "linux", target_os = "windows"),
    target_arch = "x86_64"
))]
fn cuda_provider(device: Device) -> Result<Option<ExecutionProviderDispatch>, ort::Error> {
    use ort::ep::{ExecutionProvider, CUDA};
    Ok(match device {
        Device::Cuda => Some(CUDA::default().build().error_on_failure()),
        Device::Auto if CUDA::is_available().unwrap_or(false) => {
            Some(CUDA::default().build().fail_silently())
        }
        _ => None,
    })
}

#[cfg(not(all(
    any(target_os = "linux", target_os = "windows"),
    target_arch = "x86_64"
)))]
fn cuda_provider(device: Device) -> Result<Option<ExecutionProviderDispatch>, ort::Error> {
    match device {
        Device::Cuda => Err(ort::Error::new("CUDA is not supported on this platform")),
        _ => Ok(None),
    }
}

/// Open an ONNX model file and return a ready-to-run [`Session`].
///
/// Returns `ort::Error` directly so callers can map it into their own domain
/// error type (`TextToSpeechError::Ort`, `SpeechToTextError::Ort`, …) using `?` plus a
/// `From<ort::Error>` impl.
pub fn load_session(path: &Path, device: Device) -> Result<Session, ort::Error> {
    let environment = ENVIRONMENT
        .as_ref()
        .map_err(|e| ort::Error::new(format!("failed to create ONNX Runtime environment: {e}")))?;

    Session::builder(environment)?
        .with_log_level(ort::logging::LogLevel::Warning)?
        .with_execution_providers(execution_providers(device)?)?
        .commit_from_file(path)
}
