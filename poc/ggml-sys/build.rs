use std::env;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=ggml");
    println!("cargo:rerun-if-changed=wrapper.h");

    let target = env::var("TARGET").expect("Cargo must set TARGET");
    let macos = target.contains("apple-darwin");
    let destination = build_ggml(macos);

    println!(
        "cargo:rustc-link-search=native={}",
        destination.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        destination.join("lib64").display()
    );
    println!("cargo:rustc-link-lib=static=ggml");
    println!("cargo:rustc-link-lib=static=ggml-cpu");
    if macos {
        println!("cargo:rustc-link-lib=static=ggml-metal");
    }
    println!("cargo:rustc-link-lib=static=ggml-base");

    if macos {
        println!("cargo:rustc-link-lib=c++");
        for framework in ["Accelerate", "Foundation", "Metal", "MetalKit"] {
            println!("cargo:rustc-link-lib=framework={framework}");
        }
    } else if target.contains("linux") {
        println!("cargo:rustc-link-lib=stdc++");
        println!("cargo:rustc-link-lib=dl");
        println!("cargo:rustc-link-lib=m");
    }

    generate_bindings(Path::new("ggml"));
}

fn build_ggml(macos: bool) -> PathBuf {
    cmake::Config::new("ggml")
        .define("BUILD_SHARED_LIBS", "OFF")
        .define("GGML_BACKEND_DL", "OFF")
        .define("GGML_BLAS", "OFF")
        .define("GGML_BUILD_EXAMPLES", "OFF")
        .define("GGML_BUILD_TESTS", "OFF")
        .define("GGML_CPU", "ON")
        .define("GGML_METAL", if macos { "ON" } else { "OFF" })
        .define("GGML_METAL_EMBED_LIBRARY", "ON")
        .define("GGML_METAL_NDEBUG", if macos { "ON" } else { "OFF" })
        .define("GGML_NATIVE", "OFF")
        .define("GGML_OPENMP", "OFF")
        .profile("Release")
        .build()
}

fn generate_bindings(source: &Path) {
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .clang_arg(format!("-I{}", source.join("include").display()))
        .allowlist_function("ggml_.*")
        .allowlist_function("gguf_.*")
        .allowlist_type("ggml_.*")
        .allowlist_type("gguf_.*")
        .allowlist_var("GGML_.*")
        .allowlist_var("GGUF_.*")
        .derive_default(true)
        .prepend_enum_name(false)
        .generate_comments(false)
        .layout_tests(false)
        .generate()
        .expect("failed to generate GGML bindings");

    let output = PathBuf::from(env::var("OUT_DIR").expect("Cargo must set OUT_DIR"));
    bindings
        .write_to_file(output.join("bindings.rs"))
        .expect("failed to write GGML bindings");
}
