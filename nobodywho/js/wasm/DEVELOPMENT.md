# JavaScript bindings

WASM for running LLMs in Node.js or in the browser.

Uses [UniFFI](https://mozilla.github.io/uniffi-rs/) to generate Emscripten bindings from Rust, with
[uniffi-bindgen-react-native](https://github.com/aspect-build/aspect-build) as the code generator.

## Prerequisites

- Nightly Rust toolchain
- Emscripten SDK
- Node.js 22+

## Build

```toml
CFLAGS_wasm32_unknown_emscripten = "-fPIC"
CXXFLAGS_wasm32_unknown_emscripten = "-fPIC"

[target.'cfg(target_os = "emscripten")']
# Needed because we're compiling C++ sources
linker = "em++"
rustflags = [
    "-Clink-arg=--embed-file=/Users/madsmtm/Desktop/nobodywho/models/Qwen_Qwen3-0.6B-Q4_K_M.gguf@/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
]
runner = "node"
```

For testing:

```sh
cd nobodywho/js/wasm
npm run build-uniffi
npm test
```

## Regenerate UNIFFI bindings

**Regenerate when:** Rust API changes — adding/removing/renaming functions, types, errors, or changing their signatures in `uniffi/src/lib.rs`.

```bash
cd nobodywho/js/wasm
cargo build -pnobodywho-uniffi
npm run generate-bindings -- ../target/debug/libnobodywho_uniffi.a
```

(This re-generates using the host uniffi package, which is valid as long as we assume that there's no `cfg`-guards in our UniFFI bindings that would make them differ on WASM).
