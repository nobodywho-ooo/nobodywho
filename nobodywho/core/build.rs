fn main() {
    println!("cargo:rerun-if-env-changed=OPENCL_LIBRARY");
    println!("cargo:rerun-if-env-changed=ORT_LIB_PATH");
    println!("cargo:rustc-check-cfg=cfg(nobodywho_static_ort)");
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("android") => link_android_opencl_shim(),
        Ok("emscripten") => link_emscripten_onnxruntime(),
        _ => {}
    }
}

fn link_android_opencl_shim() {
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

/// ort-sys links nothing on Emscripten (`alternative-backend`), so link the ONNX Runtime in `ORT_LIB_PATH` ourselves, if given.
fn link_emscripten_onnxruntime() {
    let Some(dir) = std::env::var_os("ORT_LIB_PATH").filter(|dir| !dir.is_empty()) else {
        return;
    };
    println!(
        "cargo:rustc-link-search=native={}",
        std::path::Path::new(&dir).display()
    );
    println!("cargo:rustc-link-lib=static=onnxruntime");
    println!("cargo:rustc-cfg=nobodywho_static_ort");
}
