use lib_flutter_rust_bridge_codegen::codegen;

fn main() {
    println!("cargo:rerun-if-changed=.");

    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();

    // link c++ standard library on macOS
    if target.contains("macos") || target.contains("darwin") {
        println!("cargo:rustc-link-lib=c++");
    }

    // emscripten-specific linker flags for the web bin target only
    if target == "emscripten" {
        let bin = "nobodywho_flutter_web";
        // Tell emscripten to invoke wasm-bindgen on the linked .wasm,
        // which auto-exports all #[wasm_bindgen] symbols and generates JS bindings
        println!("cargo:rustc-link-arg-bin={bin}=-sWASM_BINDGEN");
        // Allow memory growth for large models
        println!("cargo:rustc-link-arg-bin={bin}=-sALLOW_MEMORY_GROWTH=1");
        // Wrap output in a module factory function
        println!("cargo:rustc-link-arg-bin={bin}=-sMODULARIZE=1");
        println!("cargo:rustc-link-arg-bin={bin}=-sEXPORT_NAME='createNobodyWhoModule'");
        // Export externref intrinsics required by wasm-bindgen's externref transform.
        // Rust 1.82+ enables reference-types by default for wasm32 targets; wasm-bindgen
        // compiles these functions (gated on wbg_reference_types cfg) but wasm-ld may
        // dead-code-eliminate them since nothing calls them directly in the Rust code.
        // wasm-bindgen-cli needs them exported so it can rewrite externref operations.
        println!("cargo:rustc-link-arg-bin={bin}=-Wl,--export=__externref_table_alloc");
        println!("cargo:rustc-link-arg-bin={bin}=-Wl,--export=__externref_table_dealloc");
        println!("cargo:rustc-link-arg-bin={bin}=-Wl,--export=__externref_drop_slice");
    }

    if std::env::var("NOBODYWHO_SKIP_CODEGEN").is_ok() {
        println!(
            "cargo:warning=Skipping codegen due to NOBODYWHO_SKIP_CODEGEN environment variable"
        );
        return;
    }

    // generate bot hrust and dart interop code
    let config = codegen::Config {
        rust_input: Some("crate".to_string()),
        rust_root: Some(".".to_string()),
        rust_preamble: Some(
            "use flutter_rust_bridge::Rust2DartSendError;\nuse nobodywho::errors::*;\nuse nobodywho::chat::Message;\nuse serde_json::Value;".to_string(),
        ),
        dart_output: Some("../nobodywho/lib/src/rust".to_string()),
        dart_entrypoint_class_name: Some("NobodyWho".to_string()),
        stop_on_error: Some(true),
        ..Default::default()
    };

    codegen::generate(config, codegen::MetaConfig::default()).expect("Failed generating dart code");

    // Run build_runner to generate .freezed.dart files from the @freezed annotations
    // that flutter_rust_bridge emits in lib.dart
    let status = std::process::Command::new("flutter")
        .args([
            "pub",
            "run",
            "build_runner",
            "build",
            "--delete-conflicting-outputs",
        ])
        .current_dir("../nobodywho")
        .status()
        .expect("Failed to run dart build_runner");

    assert!(status.success(), "dart run build_runner build failed");
}
