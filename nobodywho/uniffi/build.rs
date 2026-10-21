fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    // Tell the Emscripten linker (em++ in our case) the options we need in
    // the JS package's WASM build (`npm run wasm` in `js/`).
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "emscripten" {
        // Raise the stack size, our stacks can apparently get pretty deep.
        println!("cargo:rustc-link-arg-cdylib=-sSTACK_SIZE=524288");

        // Allowing memory growth is important when we actually want to load
        // a model.
        println!("cargo:rustc-link-arg-cdylib=-sALLOW_MEMORY_GROWTH=1");

        // Link to the Dawn WebGPU port, as needed by llama.cpp.
        println!("cargo:rustc-link-arg-cdylib=--use-port=emdawnwebgpu");

        // Wrap output in an async ES6 module factory. This is nicer when
        // using it in a bundler setup (such as ours).
        println!("cargo:rustc-link-arg-cdylib=-sMODULARIZE=1");
        println!("cargo:rustc-link-arg-cdylib=-sEXPORT_ES6=1");

        // uniffi-bindgen-react-native wants to add additional entries to the
        // function table, and needs access to the WASM exports.
        println!("cargo:rustc-link-arg-cdylib=-sALLOW_TABLE_GROWTH=1");
        println!("cargo:rustc-link-arg-cdylib=-sEXPORTED_RUNTIME_METHODS=wasmExports");

        // HACK: Usually, when building a cdylib, Cargo emits a standalone
        // WASM file, without a `.js` file. We want the `.js` file though, so
        // this hackily requests that rustc creates that, and places it and
        // the `.wasm` file in the `js/build/` directory, from where
        // `js/src/index.wasm.ts` imports it.
        println!("cargo:rustc-link-arg-cdylib=-sSIDE_MODULE=0");
        let manifest_dir =
            std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
        let out_dir = manifest_dir.parent().unwrap().join("js").join("build");
        std::fs::create_dir_all(&out_dir).unwrap();
        println!("cargo:rustc-link-arg-cdylib=-o");
        println!(
            "cargo:rustc-link-arg-cdylib={}",
            out_dir.join("uniffi.js").display()
        );
    }
}
