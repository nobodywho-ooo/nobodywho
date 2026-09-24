use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android") {
        return;
    }

    // ggml-opencl is compiled against android/opencl-api.h through the CMake
    // hook prepare-gpu.sh exports, so its OpenCL calls resolve to the table in
    // src/opencl.rs rather than to libOpenCL.so.
    println!("cargo:rerun-if-env-changed=CMAKE_PROJECT_INCLUDE");
    env::var("CMAKE_PROJECT_INCLUDE")
        .expect("Android builds need the environment from android/prepare-gpu.sh");

    // Generate the table from the list that opencl-api.h declares.
    let list = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap())
        .join("../android/opencl-functions.inc");
    println!("cargo:rerun-if-changed={}", list.display());
    let text = fs::read_to_string(&list).expect("read android/opencl-functions.inc");
    let names: Vec<&str> = text
        .lines()
        .filter_map(|line| line.trim().strip_prefix("X(")?.strip_suffix(')'))
        .collect();
    let out = PathBuf::from(env::var("OUT_DIR").unwrap()).join("opencl_functions.rs");
    fs::write(out, format!("opencl_table!({});\n", names.join(", "))).unwrap();

    println!("cargo:rustc-link-lib=dl");
}
