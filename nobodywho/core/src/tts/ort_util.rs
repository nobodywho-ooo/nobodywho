use crate::errors::TtsError;
use crate::tts::{ort_execution_providers, TtsDevice};
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use std::path::Path;

pub(super) fn load_session(path: &Path, device: TtsDevice) -> Result<Session, TtsError> {
    // ort's typestate builder yields `Error<SessionBuilder>` for recovery; strip
    // the recovery slot so we can convert to our `From<ort::Error>` variant.
    let strip = |e: ort::Error<SessionBuilder>| ort::Error::new(e.to_string());
    Ok(SessionBuilder::new()?
        .with_log_level(ort::logging::LogLevel::Warning)
        .map_err(strip)?
        .with_execution_providers(ort_execution_providers(device))
        .map_err(strip)?
        .commit_from_file(path)?)
}
