fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_dir = manifest_dir.parent().unwrap();

    // Tell the Emscripten linker (em++ in our case) the options we need in
    // our js package.
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "emscripten" {
        // Raise the stack size, our stacks can apparently get pretty deep.
        println!("cargo:rustc-link-arg-cdylib=-sSTACK_SIZE=524288");

        // Allowing memory growth is important when we actually want to load
        // a model.
        println!("cargo:rustc-link-arg-cdylib=-sALLOW_MEMORY_GROWTH=1");

        // Link to the Dawn WebGPU port, as needed by llama.cpp.
        println!("cargo:rustc-link-arg-cdylib=--use-port=emdawnwebgpu");

        // Wrap output in an async ES6 module. This is nicer when using it
        // in a bundler setup (such as ours).
        println!("cargo:rustc-link-arg-cdylib=-sMODULARIZE=1");
        println!("cargo:rustc-link-arg-cdylib=-sEXPORT_ES6=1");

        // HACK: Usually, when building a cdylib, Cargo emits a standalone
        // WASM file, without a `.js` file. We want the `.js` file though, so
        // this hackily requests that rustc creates that, and places it and
        // the `.wasm` file in `js/src/` directory.
        println!("cargo:rustc-link-arg-cdylib=-sSIDE_MODULE=0");
        let uniffi_js = workspace_dir.join("js").join("src").join("uniffi.js");
        println!("cargo:rustc-link-arg-cdylib=-o");
        println!("cargo:rustc-link-arg-cdylib={}", uniffi_js.display());

        // uniffi-bindgen-react-native wants to add additional entries to the
        // symbol table, so allow that here.
        println!("cargo:rustc-link-arg-cdylib=-sALLOW_TABLE_GROWTH=1");
        println!("cargo:rustc-link-arg-cdylib=-sEXPORTED_RUNTIME_METHODS=wasmExports");
    }
}
