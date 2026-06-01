// Emscripten link flags for the pthreads-enabled wasm build.
// Only fire when the target is wasm32-unknown-emscripten — native
// builds ignore this block.

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

    // -sWASM_BINDGEN is INTENTIONALLY NOT SET here. We used to rely on
    // emcc-wbg's `-sWASM_BINDGEN` post-link step to auto-invoke
    // wasm-bindgen-cli on the linked wasm, but that step runs in
    // BUNDLER output mode (the `__wasm_bindgen_emscripten_marker` custom
    // section that switches it to Emscripten mode doesn't survive the
    // wasm-ld pass on this target — `#[link_section]` data is preserved
    // as symbols but not as a wasm custom section). The auto-invocation
    // then strips the wasm-bindgen descriptor functions from the wasm
    // and emits bundler-shape JS files we can't use, so by the time
    // js/scripts/build-pkg-emscripten.sh wants to re-process the wasm
    // for Emscripten output there's nothing left to process.
    //
    // The script instead:
    //   1. cargo build → target/.../nobodywho_js.wasm (still has
    //      descriptors)
    //   2. inject-emscripten-marker.py → inserts the custom section
    //   3. wasm-bindgen-cli manually → emits library_bindgen.js
    //   4. emcc --post-link → loader + final wasm
    // See js/scripts/build-pkg-emscripten.sh for the full pipeline.

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

    // Expose Emscripten's `Module.FS` (MEMFS API) and `Module.SYSCALLS`
    // (internal syscall plumbing) on the JS module object. Path A uses
    // both:
    //   - `Module.FS.writeFile(path, uint8)` from the JS side to land
    //     image / audio / mmproj bytes into MEMFS at content-hashed
    //     paths.
    //   - `Module.FS.open(path, flags, mode)` from inside the wasm via
    //     `js_sys::Reflect` — called by the strong `__syscall_openat`
    //     override in src/syscall_imports.rs that satisfies libc fopen
    //     against MEMFS paths.
    //   - `Module.SYSCALLS.writeStat(buf, stat)` from the wasm — used
    //     by the `__syscall_stat64` / `_lstat64` / `_fstat64` overrides
    //     to write the Emscripten struct stat layout without
    //     duplicating it in Rust.
    println!("cargo:rustc-link-arg-cdylib=-sEXPORTED_RUNTIME_METHODS=FS,SYSCALLS");

    // Force-include Emscripten's filesystem implementation. Without
    // FORCE_FILESYSTEM emcc generates no FS JS code at all — even
    // though we'd still get the wasm-side `__syscall_openat` (via our
    // override), there'd be no `Module.FS` object on the JS side to
    // route to. With FORCE_FILESYSTEM=1 the FS namespace is present
    // and our strong override can use it.
    println!("cargo:rustc-link-arg-cdylib=-sFORCE_FILESYSTEM=1");

    // Force-keep libc stdio in the wasm against wasm-ld's
    // --gc-sections pass. Without these the linker decides nothing in
    // the Rust crate directly references fopen / fread / etc. (the
    // C/C++ side of llama.cpp does, but those references are in
    // archive members that --gc-sections can elide as "unreachable").
    // `-Wl,--export=` is per-symbol — it keeps the symbol alive AND
    // exports it without disturbing the wasm-bindgen-generated export
    // list (which `-sEXPORTED_FUNCTIONS=` would clobber as a SET op).
    for sym in [
        "fopen", "fclose", "fread", "fwrite", "fseek", "ftell", "feof", "ferror",
    ] {
        println!("cargo:rustc-link-arg-cdylib=-Wl,--export={sym}");
    }

    // The file-open syscalls themselves (`__syscall_openat` /
    // `_stat64` / `_lstat64` / `_fstat64` / `_newfstatat`) are
    // provided as STRONG Rust overrides in src/syscall_imports.rs.
    // Emscripten's `system/lib/standalone/standalone.c` declares
    // these as WEAK stubs that return -EPERM (for openat) or -ENOSYS
    // (for stat); wasm-ld is happy to satisfy our references against
    // the weak stubs and never emits the syscalls as wasm imports.
    // The strong Rust overrides win symbol resolution and route to
    // Module.FS via `js_sys::Reflect`, completing the libc fopen
    // chain into our JS-populated MEMFS.

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

    // Enable Emscripten pthreads. llama.cpp uses pthreads for compute
    // parallelism (ggml threadpool), and the Rust core uses
    // std::thread::spawn (mapped to pthread_create by emscripten).
    //
    // Browser deployment requirement: the serving origin must set
    //   Cross-Origin-Opener-Policy: same-origin
    //   Cross-Origin-Embedder-Policy: credentialless   (or require-corp)
    // for SharedArrayBuffer to be available. credentialless is the most
    // forgiving value (cross-origin resources load without sending their
    // own CORP headers); require-corp also works.
    println!("cargo:rustc-link-arg-cdylib=-pthread");

    println!("cargo:rustc-link-arg-cdylib=-sDEFAULT_PTHREAD_STACK_SIZE=2MB");
}
