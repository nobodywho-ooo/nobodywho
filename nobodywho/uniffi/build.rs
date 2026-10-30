fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    let manifest_dir = std::path::PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let workspace_dir = manifest_dir.parent().unwrap();

    // Tell the Emscripten linker (em++ in our case) the options we need in
    // the JS package's WASM build (`npm run wasm` in `js/`).
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap() == "emscripten" {
        // Wrap output in an async ES6 module factory. This is nicer when
        // using it in a bundler setup (such as ours).
        println!("cargo:rustc-link-arg-cdylib=-sMODULARIZE=1");
        println!("cargo:rustc-link-arg-cdylib=-sEXPORT_ES6=1");

        // uniffi-bindgen-react-native wants to add additional entries to the
        // function table, and needs access to the WASM exports.
        //
        // The other exports are needed to load file systems.
        println!("cargo:rustc-link-arg-cdylib=-sALLOW_TABLE_GROWTH=1");
        println!(
            "cargo:rustc-link-arg-cdylib=-sEXPORTED_RUNTIME_METHODS=wasmExports,FS,NODEFS,IDBFS,addRunDependency,removeRunDependency"
        );

        // HACK: Usually, when building a cdylib, Cargo emits a standalone
        // WASM file, without a `.js` file. We want the `.js` file though, so
        // this hackily requests that rustc creates that, and places it and
        // the `.wasm` file in `js/src/` directory.
        //
        // This unfortunately does break Cargo's change-detection mechanism
        // somewhat, so rebuilding will end up relinking even if nothing
        // changed. I'm not aware of a way to avoid this.
        println!("cargo:rustc-link-arg-cdylib=-sSIDE_MODULE=0");
        let uniffi_js = workspace_dir.join("js").join("src").join("uniffi.js");
        println!("cargo:rustc-link-arg-cdylib=-o");
        println!("cargo:rustc-link-arg-cdylib={}", uniffi_js.display());
    }
}
