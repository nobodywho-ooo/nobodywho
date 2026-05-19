mod chatterbox;
mod chatterbox_roest;
mod kokoro;
mod piper;

// Backend structs — visible within the tts module tree (used by the factory).
pub(in crate::tts) use chatterbox::ChatterboxBackend;
pub(in crate::tts) use chatterbox_roest::RoestBackend;
pub(in crate::tts) use kokoro::KokoroBackend;
pub(in crate::tts) use piper::PiperBackend;

// Config structs — public API, re-exported from tts/mod.rs.
pub use chatterbox::ChatterboxConfig;
pub use chatterbox_roest::RoestConfig;
pub use kokoro::KokoroConfig;
pub use piper::PiperConfig;
