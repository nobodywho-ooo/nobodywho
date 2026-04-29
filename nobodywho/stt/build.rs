fn main() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if matches!(target_os.as_str(), "linux" | "android") {
        let dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        println!(
            "cargo:rustc-cdylib-link-arg=-Wl,--version-script={}/symbols.map",
            dir
        );
        println!("cargo:rustc-cdylib-link-arg=-Wl,-Bsymbolic");
    }
}
