mod whisper;

pub use whisper::WhisperConfig;
pub(in crate::stt) use whisper::{required_files as whisper_required_files, WhisperBackend};
