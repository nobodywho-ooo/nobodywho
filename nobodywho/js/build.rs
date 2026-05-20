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

    // Run wasm-bindgen-cli during emcc's link step. Auto-exports all
    // #[wasm_bindgen] symbols + emits the JS glue and .d.ts — same outputs
    // the standalone wasm-bindgen-cli would produce, folded into the
    // Emscripten loader. Requires walkingeyerobot's emscripten fork
    // (PR emscripten-core/emscripten#23493) on PATH and `EM_WASM_BINDGEN`
    // pointing at a stock wasm-bindgen-cli on the host.
    println!("cargo:rustc-link-arg-cdylib=-sWASM_BINDGEN");

    // rustc passes `-shared` to emcc for cdylib output, which triggers
    // SIDE_MODULE=1 in Emscripten and emits dynamic-linking-style imports
    // (`env.__stack_pointer`, `GOT.func.*`, …). wasm-bindgen-cli's
    // interpreter expects a "main module" with a defined `__stack_pointer`
    // global; on a side module it panics with `no entry found for key`.
    // Force a static main-module link so wasm-bindgen-cli can process the
    // output afterwards.
    println!("cargo:rustc-link-arg-cdylib=-sSIDE_MODULE=0");
    println!("cargo:rustc-link-arg-cdylib=-sMAIN_MODULE=0");

// Allow memory growth so GGUF loads aren't capped at Emscripten's
    // default 16 MB heap. MAXIMUM_MEMORY bumps the ceiling from the
    // default 2 GB to 4 GB — the hard cap for wasm on 32-bit browser
    // tabs. Loading a ~500 MB GGUF needs the raw bytes plus llama.cpp's
    // working set (context KV cache, scratch buffers), which blows past
    // 2 GB when ALLOW_MEMORY_GROWTH hits the default ceiling.
    println!("cargo:rustc-link-arg-cdylib=-sALLOW_MEMORY_GROWTH=1");
    println!("cargo:rustc-link-arg-cdylib=-sMAXIMUM_MEMORY=4GB");

    // Emscripten's default stack is 64 KB. minijinja's recursive-descent
    // chat-template parser (parse_math2 → parse_pow → parse_unary →
    // parse_postfix → …) overflows it on the first `chat.ask()` call,
    // producing a wasm "memory access out of bounds" trap inside
    // emscripten_builtin_malloc as the stack pointer dips below 0. 8 MB
    // matches native default and is well within our 4 GB memory budget.
    println!("cargo:rustc-link-arg-cdylib=-sSTACK_SIZE=8MB");

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

    // Allow undefined symbols at static link — they become wasm imports
    // that JS stubs at instantiation time. Needed because:
    //   * the wasm-emscripten branch of llama-cpp-sys-2 compiles out
    //     `mtmd` C++ but keeps the Rust bindgen declarations, leaving
    //     `mtmd_*` symbols unresolved;
    //   * C++ exception-handling intrinsics (`__resumeException`, …)
    //     are not statically linked unless `-fwasm-exceptions` is on.
    // Under SIDE_MODULE=1 this was implicit; in a main-module link we have
    // to opt in.
    println!("cargo:rustc-link-arg-cdylib=-sERROR_ON_UNDEFINED_SYMBOLS=0");

    // Force compiler-rt's stack_ops.S into the link. wasm-bindgen's
    // post-process step inserts calls to these from invoke_xxx shims in
    // the generated library JS, but at our `cargo build` stage nothing in
    // the input references them directly, so wasm-ld would leave them
    // out and the runtime would abort with
    //   Aborted(missing function: emscripten_stack_get_current)
    // on the first JS->wasm closure call.
    // `--export` pulls from libcompiler_rt AND marks as wasm export so the
    // Emscripten JS-side `_emscripten_stack_get_current` stub is replaced
    // with a passthrough to the wasm function. `--undefined` alone wasn't
    // enough — wasm-ld emitted the symbols then dropped them again.
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=emscripten_stack_get_current");
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=_emscripten_stack_restore");
    // emcc's --post-link asserts emscripten_stack_get_end is exported.
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=emscripten_stack_get_end");
    // emcc's stackCheckInit() runtime calls emscripten_stack_init() at
    // module instantiation; without the export the JS loader throws
    // ReferenceError before any user code runs.
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=emscripten_stack_init");

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
