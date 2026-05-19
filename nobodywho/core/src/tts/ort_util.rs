use crate::errors::TtsError;
use crate::tts::{ort_execution_providers, TtsDevice};
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};
use ort::session::Session;
use std::path::Path;

/// Build an `ort::Session` from an ONNX file.
///
/// `disable_optimization` forces `GraphOptimizationLevel::Disable` for models
/// whose exported graphs break under ORT's default fusion passes.
pub(super) fn load_session(
    path: &Path,
    device: TtsDevice,
    disable_optimization: bool,
) -> Result<Session, TtsError> {
    let mut builder = SessionBuilder::new()
        .map_err(|e| TtsError::Init(format!("ort session builder: {e}")))?
        .with_log_level(ort::logging::LogLevel::Warning)
        .map_err(|e| TtsError::Init(format!("ort log level: {e}")))?;

    if disable_optimization {
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|e| TtsError::Init(format!("ort optimization level: {e}")))?;
    }

    builder
        .with_execution_providers(ort_execution_providers(device))
        .map_err(|e| TtsError::Init(format!("ort execution providers: {e}")))?
        .commit_from_file(path)
        .map_err(|e| TtsError::Init(format!("ort load model {}: {e}", path.display())))
}
