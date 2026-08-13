use std::ffi::OsStr;
use std::path::{Path, PathBuf};

fn files_in(directory: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("reading {}: {error}", directory.display()))
        .map(|entry| entry.expect("reading runtime file").path())
        .filter(|path| path.is_file() && path.extension() == Some(OsStr::new(extension)))
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Picks the first candidate that exists. CMake's GNUInstallDirs resolves the
/// libdir per platform — `lib64` on some Linux distros, `lib` on others — so the
/// install location cannot be hardcoded. llama-cpp-sys-2 emits link-search
/// entries for both and probes the same pair for its ggml cmake dir.
fn first_dir(candidates: &[PathBuf]) -> PathBuf {
    if let Some(found) = candidates.iter().find(|path| path.is_dir()) {
        return found.clone();
    }
    let parent = candidates[0]
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let present = std::fs::read_dir(&parent)
        .map(|entries| {
            entries
                .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_else(|error| format!("<unreadable: {error}>"));
    panic!(
        "no llama runtime library directory found.\n  looked for: {}\n  {} contains: {present}",
        candidates
            .iter()
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        parent.display(),
    );
}

fn filename(path: &Path) -> &str {
    path.file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_else(|| panic!("invalid runtime filename: {}", path.display()))
}

fn install(source: &Path, destination: &Path) {
    if destination.exists() {
        std::fs::remove_file(destination).unwrap();
    }
    let source = std::fs::canonicalize(source).unwrap();
    if std::fs::hard_link(&source, destination).is_err() {
        std::fs::copy(source, destination).unwrap();
    }
}

fn android_cxx_runtime() -> PathBuf {
    let output = cc::Build::new()
        .cpp(true)
        .get_compiler()
        .to_command()
        .arg("--print-file-name=libc++_shared.so")
        .output()
        .expect("locating Android libc++");
    assert!(output.status.success(), "locating Android libc++ failed");
    let path = PathBuf::from(String::from_utf8(output.stdout).unwrap().trim());
    assert!(
        path.is_file(),
        "Android libc++ not found at {}",
        path.display()
    );
    path
}

fn main() {
    if !cfg!(feature = "dynamic-llama") {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let profile = std::env::var("PROFILE").unwrap();
    let profile_dir = out_dir
        .ancestors()
        .find(|path| path.file_name() == Some(OsStr::new(&profile)))
        .expect("Cargo profile directory not found");

    let backends_dir = PathBuf::from(
        std::env::var("DEP_LLAMA_BACKENDS_DIR").expect("llama backend directory is set"),
    );
    let llama_out = backends_dir.parent().unwrap();
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap();
    let target_vendor = std::env::var("CARGO_CFG_TARGET_VENDOR").unwrap();
    let (library_dir, library_extension) = if target_os == "windows" {
        (first_dir(&[llama_out.join("bin")]), "dll")
    } else if target_vendor == "apple" {
        (first_dir(&[llama_out.join("lib")]), "dylib")
    } else {
        (
            first_dir(&[llama_out.join("lib64"), llama_out.join("lib")]),
            "so",
        )
    };

    let mut libraries = files_in(&library_dir, library_extension);
    let backends = files_in(
        &backends_dir,
        if target_os == "windows" { "dll" } else { "so" },
    );
    assert!(!libraries.is_empty(), "no llama runtime libraries found");
    assert!(!backends.is_empty(), "no llama backend modules found");
    let cxx_runtime = (target_os == "android").then(android_cxx_runtime);
    libraries.extend(cxx_runtime.iter().cloned());
    libraries.sort_by_key(|path| path.file_name().map(OsStr::to_owned));

    for directory in [
        profile_dir.to_path_buf(),
        profile_dir.join("deps"),
        profile_dir.join("examples"),
    ] {
        if directory.is_dir() {
            for source in &backends {
                install(source, &directory.join(filename(source)));
            }
        }
    }
    if let Some(source) = &cxx_runtime {
        install(source, &profile_dir.join(filename(source)));
    }

    let runtime_dir = profile_dir.join("nobodywho-runtime");
    if runtime_dir.exists() {
        std::fs::remove_dir_all(&runtime_dir).unwrap();
    }
    std::fs::create_dir(&runtime_dir).unwrap();
    for source in libraries.iter().chain(&backends) {
        let destination = runtime_dir.join(filename(source));
        assert!(!destination.exists(), "duplicate runtime file");
        install(source, &destination);
    }

    let manifest = serde_json::json!({
        "libraries": libraries.iter().map(|path| filename(path)).collect::<Vec<_>>(),
        "backends": backends.iter().map(|path| filename(path)).collect::<Vec<_>>(),
    });
    std::fs::write(
        runtime_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
}
