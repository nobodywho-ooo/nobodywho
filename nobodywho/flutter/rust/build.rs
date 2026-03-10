use lib_flutter_rust_bridge_codegen::codegen;

fn main() {
    println!("cargo:rerun-if-changed=.");

    // link c++ standard library on macOS
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target.contains("macos") || target.contains("darwin") {
        println!("cargo:rustc-link-lib=c++");
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
}
