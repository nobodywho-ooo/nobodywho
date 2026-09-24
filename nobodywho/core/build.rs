fn main() {
    println!("cargo:rerun-if-env-changed=OPENCL_LIBRARY");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    // llama-cpp-sys builds ggml-opencl but leaves the OpenCL link to us.
    // Bundle the forwarding shim, never a mandatory dependency on a vendor .so.
    let library =
        std::path::PathBuf::from(std::env::var("OPENCL_LIBRARY").expect(
            "Android builds require the static OpenCL shim; run android/prepare-gpu.sh first",
        ));
    assert!(library.is_absolute() && library.is_file());
    assert_eq!(library.file_name().unwrap(), "libOpenCL.a");
    println!("cargo:rerun-if-changed={}", library.display());
    println!(
        "cargo:rustc-link-search=native={}",
        library.parent().unwrap().display()
    );
    println!("cargo:rustc-link-lib=static=OpenCL");
    println!("cargo:rustc-link-lib=dl");
    println!("cargo:rustc-link-lib=log");
}
