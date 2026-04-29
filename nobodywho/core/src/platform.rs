use std::path::{Path, PathBuf};

/// Returns the path to load GGML backend modules from, trying in order:
/// 1. `GGML_BACKEND_DIR` env var (explicit override)
/// 2. Directory of the currently-loaded shared library (production: Python wheel, Godot addon, Flutter)
/// 3. Compile-time OUT_DIR built by llama-cpp-sys-2 (development: cargo run, cargo test)
pub(crate) fn get_backends_path() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("GGML_BACKEND_DIR") {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = self_dir() {
        if has_backend_files(&dir) {
            return Some(dir);
        }
    }
    llama_cpp_2::llama_backend::BACKENDS_DIR.map(PathBuf::from)
}

fn has_backend_files(dir: &Path) -> bool {
    // Backend MODULE files are always .so (CMake MODULE target), even on macOS where regular
    // shared libs use .dylib. On Windows they're .dll. Match only the MODULE extension so we
    // don't confuse libggml-base.dylib (the main GGML shared lib) with a backend module.
    #[cfg(target_os = "windows")]
    let ext = ".dll";
    #[cfg(not(target_os = "windows"))]
    let ext = ".so";

    std::fs::read_dir(dir).ok().is_some_and(|mut entries| {
        entries.any(|e| {
            e.ok()
                .and_then(|e| e.file_name().into_string().ok())
                .is_some_and(|name| {
                    name.ends_with(ext)
                        && (name.starts_with("libggml-") || name.starts_with("ggml-"))
                })
        })
    })
}

/// Returns the directory of the currently-loaded shared library or executable.
/// Used to locate sibling files (backend modules, STT module) at runtime.
#[cfg(unix)]
pub(crate) fn self_dir() -> Option<PathBuf> {
    use std::ffi::CStr;
    fn anchor() {}
    unsafe {
        let mut info: libc::Dl_info = std::mem::zeroed();
        if libc::dladdr(anchor as *const libc::c_void, &mut info) == 0 || info.dli_fname.is_null() {
            return None;
        }
        CStr::from_ptr(info.dli_fname)
            .to_str()
            .ok()
            .and_then(|s| Path::new(s).parent())
            .map(|p| p.to_path_buf())
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn self_dir() -> Option<PathBuf> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };
    fn anchor() {}
    unsafe {
        let mut h_module = 0isize;
        if GetModuleHandleExW(
            GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
            anchor as *const u16,
            &mut h_module,
        ) == 0
        {
            return None;
        }
        let mut buf = vec![0u16; 32768];
        let len = GetModuleFileNameW(h_module, buf.as_mut_ptr(), buf.len() as u32);
        if len == 0 {
            return None;
        }
        let os_str = std::ffi::OsString::from_wide(&buf[..len as usize]);
        Path::new(&os_str).parent().map(|p| p.to_path_buf())
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
pub(crate) fn self_dir() -> Option<PathBuf> {
    None
}
