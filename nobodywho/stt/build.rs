fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();

    match target_os.as_str() {
        "linux" | "android" => {
            println!(
                "cargo:rustc-cdylib-link-arg=-Wl,--version-script={}/symbols.map",
                dir
            );
            println!("cargo:rustc-cdylib-link-arg=-Wl,-Bsymbolic");
        }
        "macos" | "ios" => {
            // Hide everything except the stt_module_* ABI surface so whisper's bundled
            // ggml symbols stay private to this dylib and can't be resolved against
            // llama's ggml in the host binary's two-level namespace.
            println!(
                "cargo:rustc-cdylib-link-arg=-Wl,-exported_symbols_list,{}/exported_symbols.txt",
                dir
            );
        }
        _ => {}
    }
}
