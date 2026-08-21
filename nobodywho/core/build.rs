use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn so_files(directory: &Path) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()))
        .map(|entry| entry.expect("reading runtime file").path())
        .filter(|path| path.is_file() && path.extension() == Some(OsStr::new("so")))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn main() {
    println!("cargo:rerun-if-changed=cmake/llama-build-overrides.cmake");

    if !cfg!(feature = "android-dynamic-backends")
        || std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("android")
    {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile = std::env::var("PROFILE").unwrap();
    let profile_dir = out_dir
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new(&profile)))
        .expect("Cargo profile directory");
    let backends_dir = PathBuf::from(std::env::var("DEP_LLAMA_BACKENDS_DIR").unwrap());
    let llama_out = backends_dir.parent().unwrap();
    let library_dir = [llama_out.join("lib"), llama_out.join("lib64")]
        .into_iter()
        .find(|path| path.is_dir())
        .expect("llama library directory");

    let output = cc::Build::new()
        .cpp(true)
        .get_compiler()
        .to_command()
        .arg("--print-file-name=libc++_shared.so")
        .output()
        .expect("locating libc++_shared.so");
    assert!(output.status.success(), "locating libc++_shared.so failed");
    let libcxx = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(libcxx.is_file(), "{} does not exist", libcxx.display());

    let runtime_dir = profile_dir.join("nobodywho-runtime");
    if runtime_dir.exists() {
        std::fs::remove_dir_all(&runtime_dir).unwrap();
    }
    std::fs::create_dir(&runtime_dir).unwrap();

    let backends = so_files(&backends_dir);
    assert!(
        backends.iter().any(|path| path
            .file_name()
            .unwrap()
            .as_encoded_bytes()
            .starts_with(b"libggml-cpu")),
        "no GGML CPU backends built"
    );
    let cpu_backend_names = backends
        .iter()
        .filter_map(|path| {
            let name = path.file_name()?.to_str()?;
            name.starts_with("libggml-cpu").then_some(name)
        })
        .collect::<Vec<_>>();
    println!(
        "cargo:rustc-env=NOBODYWHO_ANDROID_CPU_BACKENDS={}",
        cpu_backend_names.join(":")
    );

    for source in so_files(&library_dir)
        .into_iter()
        .chain(backends)
        .chain([libcxx])
    {
        std::fs::copy(&source, runtime_dir.join(source.file_name().unwrap())).unwrap();
    }
}
