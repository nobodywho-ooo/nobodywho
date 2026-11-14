//! Emscripten bindings for NobodyWho.
#![cfg(target_os = "emscripten")]

mod fetch;
mod log;

pub use self::fetch::*;
pub use self::log::*;
