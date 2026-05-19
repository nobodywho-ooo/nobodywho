// Emscripten link flags. Only fire when the target is
// wasm32-unknown-emscripten — native + wasm32-unknown-unknown ignore this
// block. Ported from nobodywho_old/nobodywho/flutter/rust/build.rs.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "emscripten" {
        return;
    }

    // cdylib variant of the crate needs --no-entry so emscripten doesn't
    // look for `main`. (The crate is `cdylib + rlib`; only the cdylib
    // build hits emcc's link step.)
    println!("cargo:rustc-link-arg-cdylib=--no-entry");

    // Run wasm-bindgen on the linked .wasm during emcc's link step.
    // Auto-exports all #[wasm_bindgen] symbols + emits the JS glue and
    // .d.ts — same outputs the standalone wasm-bindgen-cli would produce,
    // just folded into the Emscripten loader. Requires walkingeyerobot's
    // emscripten fork (draft upstream PR emscripten-core/emscripten#23493);
    // see flake.nix for the overlay that supplies it.
    println!("cargo:rustc-link-arg-cdylib=-sWASM_BINDGEN");

    // Allow memory growth so GGUF loads aren't capped at Emscripten's
    // default 16 MB heap. MAXIMUM_MEMORY bumps the ceiling from the
    // default 2 GB to 4 GB — the hard cap for wasm on 32-bit browser
    // tabs. Loading a ~500 MB GGUF needs the raw bytes plus llama.cpp's
    // working set (context KV cache, scratch buffers), which blows past
    // 2 GB when ALLOW_MEMORY_GROWTH hits the default ceiling.
    println!("cargo:rustc-link-arg-cdylib=-sALLOW_MEMORY_GROWTH=1");
    println!("cargo:rustc-link-arg-cdylib=-sMAXIMUM_MEMORY=4GB");

    // Wrap the output in a module factory so consumers do
    //   import createNobodyWhoModule from './nobodywho_js.js';
    //   const m = await createNobodyWhoModule();
    // instead of relying on global side effects at script-tag time.
    println!("cargo:rustc-link-arg-cdylib=-sMODULARIZE=1");
    println!("cargo:rustc-link-arg-cdylib=-sEXPORT_NAME='createNobodyWhoModule'");

    // emcc auto-populates EXPORTED_FUNCTIONS with every wasm-bindgen-related
    // symbol it discovers in the input .o files (describe functions,
    // externref intrinsics, etc.) and errors if any listed symbol isn't
    // defined. On wasm32-unknown-emscripten the externref feature isn't
    // enabled by default, so wasm-bindgen's externref.rs (gated on
    // `cfg(wbg_reference_types)`) isn't compiled and the
    // `__externref_{drop_slice,table_alloc,table_dealloc}` symbols don't
    // exist. Downgrade the missing-export error to a warning — the exports
    // are speculative and harmless when the target doesn't use them.
    println!("cargo:rustc-link-arg-cdylib=-Wno-undefined");

    // Clamp emcc's link-time optimization level so wasm-opt doesn't run.
    // At -O2/-O3, emcc runs binaryen/wasm-opt with aggressive DCE that
    // strips the `__wbindgen_start` export and every `__wbindgen_describe_*`
    // function wasm-bindgen-cli inserts post-link (those get added AFTER
    // wasm-ld by wasm-bindgen, so the normal "keep named exports" logic
    // doesn't protect them against emcc's own later optimizer pass). -O1
    // keeps Rust-side optimization (applied during rustc's own codegen)
    // while skipping the wasm-opt pass at link time. See emscripten
    // tools/link.py:302 — `should_run_binaryen_optimizer` triggers at
    // OPT_LEVEL >= 2.
    println!("cargo:rustc-link-arg-cdylib=-O1");

    // No -pthread: Emscripten pthreads conflict with wasm-bindgen-cli's
    // own thread-id injection (wasm-bindgen looks for __heap_base which
    // Emscripten doesn't expose). For off-main-thread inference, the JS
    // host spawns a Web Worker that imports this loader; the wasm stays
    // single-threaded inside that worker. See
    // llama-cpp-rs/llama-cpp-sys-2/build.rs (WasmEmscripten branch) for
    // the matching CMake config.
}
