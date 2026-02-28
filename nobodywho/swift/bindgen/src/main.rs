use camino::Utf8Path;
use std::env;
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        eprintln!("Usage: {} <udl-file> <output-dir>", args[0]);
        std::process::exit(1);
    }

    let udl_file = &args[1];
    let out_dir = &args[2];

    println!("Generating Swift bindings...");
    println!("  UDL file: {}", udl_file);
    println!("  Output dir: {}", out_dir);

    // Create output directory
    std::fs::create_dir_all(out_dir).expect("Failed to create output directory");

    // Generate Swift bindings
    let udl_path = Utf8Path::new(udl_file);

    // Use the library mode of uniffi_bindgen
    uniffi_bindgen::library_mode::generate_bindings(
        udl_path,
        None,
        &uniffi_bindgen::bindings::TargetLanguage::Swift,
        &PathBuf::from(out_dir),
        false,
    )
    .expect("Failed to generate Swift bindings");

    println!("✓ Swift bindings generated successfully");
}
