// Emscripten link flags for wasm32-unknown-emscripten builds.

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "emscripten" {
        return;
    }

    // cdylib build: --no-entry so emcc doesn't look for `main`.
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

    // GGUF loads + llama.cpp working set exceed Emscripten's 2 GB default cap.
    println!("cargo:rustc-link-arg-cdylib=-sALLOW_MEMORY_GROWTH=1");
    println!("cargo:rustc-link-arg-cdylib=-sMAXIMUM_MEMORY=4GB");

    // minijinja's recursive chat-template parser overflows Emscripten's 64 KB
    // default stack on the first ask; 8 MB matches native.
    println!("cargo:rustc-link-arg-cdylib=-sSTACK_SIZE=8MB");

    // Modularize: consumers do `const m = await createNobodyWhoModule()` instead of global side effects.
    println!("cargo:rustc-link-arg-cdylib=-sMODULARIZE=1");
    println!("cargo:rustc-link-arg-cdylib=-sEXPORT_NAME='createNobodyWhoModule'");

    // Expose Module.FS (used by JS to write media/model bytes into MEMFS and by
    // syscall_imports.rs to open them) and Module.SYSCALLS (for writeStat).
    println!("cargo:rustc-link-arg-cdylib=-sEXPORTED_RUNTIME_METHODS=FS,SYSCALLS");

    // Without FORCE_FILESYSTEM emcc emits no Module.FS JS even with our
    // syscall override — the FS namespace would be missing at runtime.
    println!("cargo:rustc-link-arg-cdylib=-sFORCE_FILESYSTEM=1");

    // Keep libc stdio alive against wasm-ld's --gc-sections (llama.cpp's
    // references are in archive members that get elided without --export).
    for sym in [
        "fopen", "fclose", "fread", "fwrite", "fseek", "ftell", "feof", "ferror",
    ] {
        println!("cargo:rustc-link-arg-cdylib=-Wl,--export={sym}");
    }

    // syscall_imports.rs provides strong overrides for __syscall_openat/stat64/fstat64,
    // beating Emscripten's weak -EPERM/-ENOSYS stubs and routing to Module.FS.

    // wasm-bindgen's externref symbols aren't emitted on this target;
    // downgrade missing-export from error to warning.
    println!("cargo:rustc-link-arg-cdylib=-Wno-undefined");

    // Undefined symbols become wasm imports: mtmd_* (compiled out in the
    // wasm branch) and C++ EH intrinsics need this to link cleanly.
    println!("cargo:rustc-link-arg-cdylib=-sERROR_ON_UNDEFINED_SYMBOLS=0");

    // compiler-rt stack ops: wasm-bindgen's invoke_xxx shims call these but
    // nothing in our cargo build references them directly — wasm-ld drops
    // them unless --export forces them in AND marks them as wasm exports.
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=emscripten_stack_get_current");
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=_emscripten_stack_restore");
    // Required by emcc --post-link and stackCheckInit() at module instantiation.
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=emscripten_stack_get_end");
    println!("cargo:rustc-link-arg-cdylib=-Wl,--export=emscripten_stack_init");

    // -O1: skip wasm-opt (runs at -O2+) which strips wasm-bindgen's post-link
    // __wbindgen_start/__wbindgen_describe_* exports via DCE.
    println!("cargo:rustc-link-arg-cdylib=-O1");

    // pthreads for ggml threadpool + std::thread::spawn. Requires
    // COOP: same-origin + COEP: credentialless (or require-corp) for SharedArrayBuffer.
    println!("cargo:rustc-link-arg-cdylib=-pthread");

    println!("cargo:rustc-link-arg-cdylib=-sDEFAULT_PTHREAD_STACK_SIZE=2MB");
}
