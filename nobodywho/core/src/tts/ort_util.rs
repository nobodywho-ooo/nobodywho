use crate::errors::TtsError;
use crate::tts::{ort_execution_providers, TtsDevice};
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use std::path::Path;

pub(super) fn load_session(path: &Path, device: TtsDevice) -> Result<Session, TtsError> {
    SessionBuilder::new()
        .map_err(|e| TtsError::Init(format!("ort session builder: {e}")))?
        .with_log_level(ort::logging::LogLevel::Warning)
        .map_err(|e| TtsError::Init(format!("ort log level: {e}")))?
        .with_execution_providers(ort_execution_providers(device))
        .map_err(|e| TtsError::Init(format!("ort execution providers: {e}")))?
        .commit_from_file(path)
        .map_err(|e| TtsError::Init(format!("ort load model {}: {e}", path.display())))
}
