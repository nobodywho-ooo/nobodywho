//! The minimal build of NobodyWho for WASM.

// When building a cdylib, Cargo emits a standalone WASM file,
// without the associated `.js` file.
//
// We want that `.js` file though, so we build a `#![no_main]` binary that
// will act as a dylib.
#![cfg_attr(target_os = "emscripten", no_main)]

// Make `rustc` see that it should expose the symbols from `nobodywho-uniffi`.
use nobodywho_uniffi as _;

#[cfg(not(target_os = "emscripten"))]
fn main() {
    panic!("not meant to be used as a binary");
}
