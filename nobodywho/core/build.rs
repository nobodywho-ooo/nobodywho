use std::path::{Path, PathBuf};

// Co-locate the ggml backend modules (CPU SIMD variants, Metal, Vulkan, …) next to our
// built binary. Under GGML_BACKEND_DL those backends are separate dlopen'd modules that
// llama-cpp-sys-2 installs into its own `out/backends` and — unlike the ggml/llama shared
// libs — does NOT relocate next to the artifact. We mirror that relocation so every
// downstream binding ships the modules beside our binding, and the runtime loader
// (llm.rs::current_dylib_dir) finds them there. Linking into the profile root AND deps/
// (as llama-cpp-sys-2 does) means `cargo build`/`cargo test` binaries are covered too, so
// the runtime has a single path with no dev/test special-case.
fn main() {
    println!("cargo:rerun-if-env-changed=DEP_LLAMA_BACKENDS_DIR");
    let Ok(backends_dir) = std::env::var("DEP_LLAMA_BACKENDS_DIR") else {
        return; // dynamic-backends not active in this build — nothing to relocate
    };
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR is set for build scripts");
    // OUT_DIR = <target>/<triple>/<profile>/build/<pkg>-<hash>/out
    let profile_dir = Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .expect("OUT_DIR has the expected depth")
        .to_path_buf();

    let modules: Vec<PathBuf> = std::fs::read_dir(&backends_dir)
        .unwrap_or_else(|e| panic!("reading backends dir {backends_dir}: {e}"))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();

    for dest in [
        profile_dir.clone(),
        profile_dir.join("deps"),
        profile_dir.join("examples"),
    ] {
        if !dest.is_dir() {
            continue;
        }
        for module in &modules {
            let dst = dest.join(module.file_name().expect("module has a file name"));
            if dst.exists() {
                continue;
            }
            // Hard-link (cheap, same filesystem as target/); fall back to copy if that fails.
            if std::fs::hard_link(module, &dst).is_err() {
                std::fs::copy(module, &dst).unwrap_or_else(|e| {
                    panic!("copying {} -> {}: {e}", module.display(), dst.display())
                });
            }
        }
    }
}
