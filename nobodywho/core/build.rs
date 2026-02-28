fn main() {
    // Generate Rust scaffolding
    uniffi::generate_scaffolding("src/nobodywho.udl").unwrap();

    // Also generate Swift bindings during build if requested
    if std::env::var("UNIFFI_GENERATE_SWIFT").is_ok() {
        let out_dir = std::env::var("OUT_DIR").unwrap();
        println!("cargo:warning=Generating Swift bindings to {}", out_dir);

        // The Swift bindings will be generated in OUT_DIR
        // Users can copy them from there to the Swift package
    }
}
