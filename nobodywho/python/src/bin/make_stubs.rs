/// This file builds stubs for the nobodywho-python crate.
/// It uses the pyo3-introspection crate to introspect the nobodywho-python crate and
/// generate stubs for the Python bindings. The script should be run after the crate is built.
///
/// Usage:
///   cargo run --bin make_stubs [--release]
use pyo3_introspection::{introspect_cdylib, module_stub_files};
use std::env;
use std::fs;
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Determine build profile (debug or release)
    let profile = if env::var("PROFILE").unwrap_or_default() == "release" {
        "release"
    } else {
        "debug"
    };

    // Determine library extension based on platform
    let lib_ext = if cfg!(target_os = "macos") {
        "dylib"
    } else if cfg!(target_os = "windows") {
        "dll"
    } else {
        "so"
    };

    // Construct library path
    let library_path = format!("../target/{}/libnobodywho_python.{}", profile, lib_ext);

    // Check if library exists
    if !PathBuf::from(&library_path).exists() {
        eprintln!("Error: Library not found at {}", library_path);
        eprintln!("Please build the project first with: cargo build [--release]");
        std::process::exit(1);
    }

    let module = introspect_cdylib(&library_path, "nobodywho")?;
    let stub_files = module_stub_files(&module);

    // Output directory (can be overridden with STUBS_DIR env var)
    let output_dir = env::var("STUBS_DIR").unwrap_or_else(|_| "nobodywho".to_string());

    println!("Generating stub files in: {}", output_dir);
    for (file_path, contents) in stub_files.clone().iter() {
        let full_path = PathBuf::from(&output_dir).join(&file_path);

        // Create parent directories if they don't exist
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        fs::write(&full_path, contents)?;
        println!("  Generated: {}", full_path.display());
    }

    println!("Done! Generated {} stub file(s)", stub_files.clone().len());
    Ok(())
}
