use std::path::PathBuf;
use std::process::Command;

fn main() {
    // On Linux, Godot's official (and nixpkgs) editor binaries embed a static
    // copy of libstdc++ and export its symbols (-rdynamic, for the crash
    // handler). The executable's exports preempt our dynamically-linked
    // libstdc++'s internal calls, so C++ code in this extension (ONNX Runtime,
    // llama.cpp) ends up running a mix of two libstdc++ versions -> heap
    // corruption ("free(): invalid size" in std::locale teardown).
    //
    // Fix: statically link libstdc++ into the extension and hide the symbols
    // (--exclude-libs ALL), so every libstdc++ call binds locally at link
    // time and can't be preempted by whatever copy the Godot binary exports.
    //
    // Dependency build scripts (llama-cpp-sys, ort) emit an explicit
    // `-lstdc++`, which `-static-libstdc++` does not override. So we also
    // place a copy of libstdc++.a in a search dir that is consulted first;
    // `-lstdc++` then resolves to the static archive.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return;
    }

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR"));
    let cxx = std::env::var("CXX").unwrap_or_else(|_| "g++".into());
    let output = Command::new(&cxx)
        .arg("-print-file-name=libstdc++.a")
        .output()
        .expect("failed to run the C++ compiler to locate libstdc++.a");
    let archive = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
    if !archive.is_absolute() || !archive.exists() {
        println!(
            "cargo:warning=libstdc++.a not found ({}); libstdc++ will be linked dynamically, \
             which crashes under Godot binaries that export their own libstdc++ symbols",
            archive.display()
        );
        return;
    }
    // Symlink, not copy: fs::copy preserves a read-only mode (e.g. from the
    // nix store), which breaks overwriting on the next build.
    let dest = out_dir.join("libstdc++.a");
    let _ = std::fs::remove_file(&dest);
    std::os::unix::fs::symlink(&archive, &dest).expect("failed to symlink libstdc++.a");

    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-arg=-static-libstdc++");
    println!("cargo:rustc-link-arg=-Wl,--exclude-libs,ALL");
}
