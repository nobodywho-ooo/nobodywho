fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Tell the Emscripten linker (em++ in our case) the options we need.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "emscripten" {
        // Raise the stack size, our stacks can apparently get pretty deep.
        println!("cargo:rustc-link-arg-bins=-sSTACK_SIZE=512KB");

        // Allowing memory growth is important when we actually want to load a model.
        println!("cargo:rustc-link-arg-bins=-sALLOW_MEMORY_GROWTH=1");

        // Wrap output in an async ES6 module factory. This is nicer when
        // using it in a bundler setup (such as ours).
        println!("cargo:rustc-link-arg-bins=-sMODULARIZE=1");
        println!("cargo:rustc-link-arg-bins=-sEXPORT_ES6=1");

        // Emscripten minifies wasm export names at -O2+ (and renames them in
        // its glue JS to match), which breaks UniFFI which looks up
        // `__ubrn_alloc` etc. by name.
        //
        // Ideally, we'd just set `-sMINIFY_WASM_EXPORT_NAMES=0`, but that's
        // internal, so as a workaround we link the exports.js library:
        // https://github.com/emscripten-core/emscripten/issues/20762
        println!("cargo:rustc-link-arg-bins=-lexports.js");

        // Link to the Dawn WebGPU port, as needed by llama.cpp.
        // println!("cargo:rustc-link-arg-bins=--use-port=emdawnwebgpu");

        // Allow setting environment variables that we read in tests.
        println!("cargo:rustc-link-arg-bins=-sNODE_HOST_ENV=1");

        // Allow accessing the host filesystem when running under Node.js.
        println!("cargo:rustc-link-arg-bins=-lnodefs.js");
        // And store in persistent storage when running in the browser.
        println!("cargo:rustc-link-arg-bins=-lidbfs.js");

        // Tell emscripten to invoke wasm-bindgen on the linked .wasm, which
        // auto-exports all #[wasm_bindgen] symbols and generates JS bindings.
        // Not needed currently, but might be needed in the future?
        // println!("cargo:rustc-link-arg-bins=-sWASM_BINDGEN");

        // TODO: rustc defaults to the legacy wasm EH encoding for the
        // Emscripten target, we should somehow use the newer encoding!
        //
        // println!("cargo:rustc-link-arg-bins=-fno-wasm-exceptions");
        // println!("cargo:rustc-link-arg-bins=-fwasm-exceptions");
        // println!("cargo:rustc-link-arg-bins=-sDISABLE_EXCEPTION_CATCHING=1");
        // println!("cargo:rustc-link-arg-bins=-exceptions");
        // println!("cargo:rustc-link-arg-bins=-fwasm-exceptions");
        // "-Cllvm-args=-wasm-use-legacy-eh=0"
        // "-Cllvm-args=-enable-emscripten-cxx-exceptions=0"
        // "-Zemscripten-wasm-eh"

        // TODO: We need to enable JSPI to get WebGPU support, but UniFFI or
        // `uniffi-bindgen-react-native` doesn't seem to support that:
        // https://github.com/jhugman/uniffi-bindgen-react-native/issues/507
        // println!("cargo:rustc-link-arg-bins=-sJSPI");

        // Alternative to JSPI.
        // This requires `GGML_WEBGPU_JSPI=OFF` (llama.cpp defaults to JSPI).
        // println!("cargo:rustc-link-arg-bins=-sASYNCIFY");

        // TODO: Enable pthreads in some configurations?
        // println!("cargo:rustc-link-arg-bins=-pthread"); // Also needs to be set in CFLAGS/CXXFLAGS, and needs -Zbuild-std and -Ctarget-feature=+atomics

        // The `@ubjs/wasm` player installs JS trampolines (panic logging,
        // callbacks) into the module's indirect function table, so the table
        // must be exported and growable. `ALLOW_TABLE_GROWTH` makes Emscripten
        // pass `--export-table --growable-table` to wasm-ld.
        println!("cargo:rustc-link-arg-bins=-sALLOW_TABLE_GROWTH=1");

        // `uniffi-bindgen-react-native` adds additional entries to the
        // function table (see above), and thus needs access to `wasmExports`.
        //
        // At `-O3`, the `wasm-metadce` would still strip the exports though,
        // we avoid that by exporting `wasmTable` too.
        //
        // The remaining exports are needed to load file systems.
        println!(
            "cargo:rustc-link-arg-bins=-sEXPORTED_RUNTIME_METHODS=wasmExports,wasmTable,FS,NODEFS,IDBFS,addRunDependency,removeRunDependency"
        );
    }
}
