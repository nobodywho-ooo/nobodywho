use crate::errors::BackendInitError;
use llama_cpp_2::llama_backend::LlamaBackend;
use std::sync::LazyLock;

static LLAMA_BACKEND: LazyLock<Result<LlamaBackend, BackendInitError>> = LazyLock::new(|| {
    #[cfg(all(feature = "android-dynamic-backends", target_os = "android"))]
    android::load_best_cpu_backend()?;

    LlamaBackend::init().map_err(|error| BackendInitError::Llama {
        reason: error.to_string(),
    })
});

pub(crate) fn backend() -> Result<&'static LlamaBackend, BackendInitError> {
    match LazyLock::force(&LLAMA_BACKEND) {
        Ok(backend) => Ok(backend),
        Err(error) => Err(error.clone()),
    }
}

#[cfg(all(feature = "android-dynamic-backends", target_os = "android"))]
mod android {
    use crate::errors::BackendInitError;
    use std::ffi::{CStr, CString};
    use std::path::Path;
    use tracing::{debug, info};

    pub(super) fn load_best_cpu_backend() -> Result<(), BackendInitError> {
        let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
        let found =
            unsafe { libc::dladdr(load_best_cpu_backend as *const libc::c_void, &mut info) };
        if found == 0 || info.dli_fname.is_null() {
            return Err(BackendInitError::LocateLibrary {
                reason: "dladdr found no containing library".into(),
            });
        }
        let own_path = unsafe { CStr::from_ptr(info.dli_fname) }
            .to_str()
            .map_err(|error| BackendInitError::LocateLibrary {
                reason: error.to_string(),
            })?;
        let dir = Path::new(own_path)
            .parent()
            .ok_or_else(|| BackendInitError::LocateLibrary {
                reason: format!("{own_path} has no parent directory"),
            })?;

        // Android can dlopen an exact `base.apk!/lib/<abi>/libfoo.so` path, but its
        // filesystem APIs cannot enumerate that pseudo-directory.
        let mut best_backend = None;
        for filename in env!("NOBODYWHO_ANDROID_CPU_BACKENDS").split(':') {
            let path = dir.join(filename);
            let path_str = path
                .to_str()
                .ok_or_else(|| BackendInitError::LocateLibrary {
                    reason: format!("{} is not valid UTF-8", path.display()),
                })?;
            let path_c =
                CString::new(path_str).map_err(|error| BackendInitError::LocateLibrary {
                    reason: error.to_string(),
                })?;
            let Some(score) = score_backend(&path_c) else {
                continue;
            };
            info!(backend = %path.display(), score, "scored GGML CPU backend");
            if score <= 0 {
                continue;
            }

            if best_backend
                .as_ref()
                .is_none_or(|(_, _, best_score)| score > *best_score)
            {
                best_backend = Some((path, path_c, score));
            }
        }
        let (backend_path, backend_path_c, score) =
            best_backend.ok_or(BackendInitError::NoLoadableCpuBackend)?;

        let backend = unsafe { llama_cpp_sys_2::ggml_backend_load(backend_path_c.as_ptr()) };
        if backend.is_null() {
            return Err(BackendInitError::RegisterCpuBackend {
                path: backend_path.display().to_string(),
            });
        }
        info!(backend = %backend_path.display(), score, "loaded GGML CPU backend");
        Ok(())
    }

    fn score_backend(path: &CStr) -> Option<i32> {
        let handle = unsafe { libc::dlopen(path.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if handle.is_null() {
            let error = unsafe { libc::dlerror() };
            let error = if error.is_null() {
                "unknown error".into()
            } else {
                unsafe { CStr::from_ptr(error) }
                    .to_string_lossy()
                    .into_owned()
            };
            debug!(backend = %path.to_string_lossy(), %error, "failed to open GGML CPU backend");
            return None;
        }

        let symbol = unsafe { libc::dlsym(handle, c"ggml_backend_score".as_ptr()) };
        let score = if symbol.is_null() {
            None
        } else {
            let score_fn: unsafe extern "C" fn() -> i32 = unsafe { std::mem::transmute(symbol) };
            Some(unsafe { score_fn() })
        };
        unsafe { libc::dlclose(handle) };
        score
    }
}
