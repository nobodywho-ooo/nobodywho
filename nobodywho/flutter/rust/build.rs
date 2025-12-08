use lib_flutter_rust_bridge_codegen::codegen;

fn main() {
    println!("cargo:rerun-if-changed=.");

    // do a special little dance for the androids
    let target = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    if target.contains("android") {
        println!("cargo:rustc-link-lib=c++_shared");

    // ...and another dance for macos
    } else if target.contains("macos") || target.contains("darwin") {
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
        rust_input: Some("crate::api".to_string()),
        rust_root: Some(".".to_string()),
        rust_preamble: Some("use flutter_rust_bridge::Rust2DartSendError;".to_string()),
        dart_output: Some("../nobodywho_dart/lib/src/rust".to_string()),
        ..Default::default()
    };

    codegen::generate(config, codegen::MetaConfig::default()).expect("Failed generating dart code");
}
