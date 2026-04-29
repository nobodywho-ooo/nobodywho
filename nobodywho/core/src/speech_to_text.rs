use crate::errors::SpeechToTextError;
use crate::llm::{TokenStream, TokenStreamAsync, WorkerGuard, WriteOutput};
use crate::platform::self_dir;
use libloading::Library;
use std::ffi::{c_char, c_void, CString};
use std::sync::{Arc, OnceLock};
use tracing::error;

const EXPECTED_ABI_VERSION: u32 = 1;

// Plain filename — used for single-arch installs (Python wheels, local builds).
#[cfg(target_os = "windows")]
const LIB_NAME: &str = "nobodywho_stt.dll";
#[cfg(target_os = "macos")]
const LIB_NAME: &str = "libnobodywho_stt.dylib";
#[cfg(not(any(target_os = "windows", target_os = "macos")))]
const LIB_NAME: &str = "libnobodywho_stt.so";

// Target-triple-specific filename — used in multi-arch release bundles (Godot addon zip,
// Flutter release dir) where multiple architectures coexist in the same directory.
// Mirrors the rename step in build.yml: libnobodywho_stt-{triple}-{profile}.ext
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LIB_NAME_RELEASE: &str = "libnobodywho_stt-x86_64-unknown-linux-gnu-release.so";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const LIB_NAME_RELEASE: &str = "libnobodywho_stt-aarch64-unknown-linux-gnu-release.so";
#[cfg(all(target_os = "macos", target_arch = "x86_64"))]
const LIB_NAME_RELEASE: &str = "libnobodywho_stt-x86_64-apple-darwin-release.dylib";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const LIB_NAME_RELEASE: &str = "libnobodywho_stt-aarch64-apple-darwin-release.dylib";
#[cfg(all(target_os = "windows", target_arch = "x86_64"))]
const LIB_NAME_RELEASE: &str = "nobodywho_stt-x86_64-pc-windows-msvc-release.dll";
#[cfg(all(target_os = "android", target_arch = "aarch64"))]
const LIB_NAME_RELEASE: &str = "libnobodywho_stt-aarch64-linux-android-release.so";
#[cfg(all(target_os = "android", target_arch = "x86_64"))]
const LIB_NAME_RELEASE: &str = "libnobodywho_stt-x86_64-linux-android-release.so";
#[cfg(not(any(
    all(target_os = "linux", target_arch = "x86_64"),
    all(target_os = "linux", target_arch = "aarch64"),
    all(target_os = "macos", target_arch = "x86_64"),
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "windows", target_arch = "x86_64"),
    all(target_os = "android", target_arch = "aarch64"),
    all(target_os = "android", target_arch = "x86_64"),
)))]
const LIB_NAME_RELEASE: &str = LIB_NAME; // fallback to plain name for unknown targets

// -- STT dylib symbol table --

struct SttSyms {
    version: unsafe extern "C" fn() -> u32,
    create: unsafe extern "C" fn(*const c_char, *const c_char, bool, *const c_char) -> *mut c_void,
    destroy: unsafe extern "C" fn(*mut c_void),
    transcribe: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
        Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
        *mut c_void,
    ) -> i32,
    last_error: unsafe extern "C" fn() -> *const c_char,
}

// SAFETY: function pointers from a loaded library are valid for the process lifetime
// (the Library is held alive in STT_LIB).
unsafe impl Send for SttSyms {}
unsafe impl Sync for SttSyms {}

static STT_LIB: OnceLock<Result<(Library, SttSyms), String>> = OnceLock::new();

fn load_stt() -> Result<&'static SttSyms, SpeechToTextError> {
    STT_LIB
        .get_or_init(try_load_stt)
        .as_ref()
        .map(|(_, syms)| syms)
        .map_err(|e| SpeechToTextError::ModuleLoad(e.clone()))
}

fn try_load_stt() -> Result<(Library, SttSyms), String> {
    let lib = open_stt_library()?;

    let syms = unsafe {
        macro_rules! sym {
            ($name:literal, $ty:ty) => {{
                let s: libloading::Symbol<$ty> = lib
                    .get($name)
                    .map_err(|e| format!("symbol {} not found: {}", stringify!($name), e))?;
                *s
            }};
        }
        SttSyms {
            version: sym!(b"stt_module_version\0", unsafe extern "C" fn() -> u32),
            create: sym!(
                b"stt_module_create\0",
                unsafe extern "C" fn(
                    *const c_char,
                    *const c_char,
                    bool,
                    *const c_char,
                ) -> *mut c_void
            ),
            destroy: sym!(b"stt_module_destroy\0", unsafe extern "C" fn(*mut c_void)),
            transcribe: sym!(
                b"stt_module_transcribe\0",
                unsafe extern "C" fn(
                    *mut c_void,
                    *const c_char,
                    Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
                    Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
                    *mut c_void,
                ) -> i32
            ),
            last_error: sym!(
                b"stt_module_last_error\0",
                unsafe extern "C" fn() -> *const c_char
            ),
        }
    };

    let got = unsafe { (syms.version)() };
    if got != EXPECTED_ABI_VERSION {
        return Err(format!(
            "version mismatch: expected {}, got {}",
            EXPECTED_ABI_VERSION, got
        ));
    }

    Ok((lib, syms))
}

fn open_stt_library() -> Result<Library, String> {
    // 1. Explicit override via environment variable.
    if let Ok(path) = std::env::var("NOBODYWHO_STT_MODULE_PATH") {
        return unsafe { Library::new(&path) }
            .map_err(|e| format!("NOBODYWHO_STT_MODULE_PATH={path}: {e}"));
    }

    // 2. Sibling to the current shared library (covers Python site-packages, Godot addon dir).
    //    Try the arch-specific release name first (multi-arch bundles like the Godot addon zip
    //    where x86_64 and aarch64 files coexist), then fall back to the plain name.
    if let Some(dir) = self_dir() {
        for name in &[LIB_NAME_RELEASE, LIB_NAME] {
            let candidate = dir.join(name);
            if candidate.exists() {
                if let Ok(lib) = unsafe { Library::new(&candidate) } {
                    return Ok(lib);
                }
            }
        }
    }

    // 3. Sibling to the running executable (covers standalone binaries, test wrappers).
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in &[LIB_NAME_RELEASE, LIB_NAME] {
                let candidate = dir.join(name);
                if candidate.exists() {
                    if let Ok(lib) = unsafe { Library::new(&candidate) } {
                        return Ok(lib);
                    }
                }
            }
        }
    }

    // 4. OS default search path (LD_LIBRARY_PATH / DYLD_LIBRARY_PATH / PATH).
    unsafe { Library::new(LIB_NAME) }
        .map_err(|e| format!("could not find {LIB_NAME} in any search path: {e}"))
}

// -- Opaque handle wrapper (send across threads) --
//
// Note: accessing `.0` inside a closure triggers RFC-2229 disjoint capture of the field
// (*mut c_void, which is !Send). Using a method call instead forces capture of the whole
// HandlePtr (which is Send).

struct HandlePtr(*mut c_void);
unsafe impl Send for HandlePtr {}

impl HandlePtr {
    fn ptr(&self) -> *mut c_void {
        self.0
    }
}

// -- Public API types --

/// Configuration for speech-to-text transcription.
#[derive(Clone, Debug, Default)]
pub struct SpeechToTextConfig {
    /// Target language code (e.g. "en", "de"). None for auto-detect.
    pub language: Option<String>,
    /// Translate output to English instead of transcribing. Default: false.
    pub translate: bool,
    /// Text to prime the decoder with domain-specific vocabulary. Default: None.
    pub initial_prompt: Option<String>,
}

/// Synchronous speech-to-text handle. Wraps [`SpeechToTextAsync`].
#[derive(Clone)]
pub struct SpeechToText {
    async_handle: SpeechToTextAsync,
}

/// Asynchronous speech-to-text handle backed by a dedicated worker thread.
#[derive(Clone)]
pub struct SpeechToTextAsync {
    guard: Arc<WorkerGuard<SttMsg>>,
}

struct SttMsg {
    audio_path: String,
    output_tx: tokio::sync::mpsc::UnboundedSender<WriteOutput>,
}

// -- Public API impl --

impl SpeechToText {
    pub fn new(model_path: String, config: SpeechToTextConfig) -> Result<Self, SpeechToTextError> {
        let async_handle = SpeechToTextAsync::new(model_path, config)?;
        Ok(Self { async_handle })
    }

    pub fn transcribe(&self, audio_path: String) -> TokenStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.async_handle.guard.send(SttMsg {
            audio_path,
            output_tx: tx,
        });
        TokenStream::new(rx)
    }
}

impl SpeechToTextAsync {
    pub fn new(model_path: String, config: SpeechToTextConfig) -> Result<Self, SpeechToTextError> {
        let syms = load_stt()?;

        let model_cstr =
            CString::new(model_path).map_err(|e| SpeechToTextError::LoadModel(e.to_string()))?;
        let language_cstr = config
            .language
            .as_deref()
            .map(CString::new)
            .transpose()
            .map_err(|e| SpeechToTextError::LoadModel(e.to_string()))?;
        let prompt_cstr = config
            .initial_prompt
            .as_deref()
            .map(CString::new)
            .transpose()
            .map_err(|e| SpeechToTextError::LoadModel(e.to_string()))?;

        let raw_handle = unsafe {
            (syms.create)(
                model_cstr.as_ptr(),
                language_cstr
                    .as_ref()
                    .map_or(std::ptr::null(), |s| s.as_ptr()),
                config.translate,
                prompt_cstr
                    .as_ref()
                    .map_or(std::ptr::null(), |s| s.as_ptr()),
            )
        };

        if raw_handle.is_null() {
            let msg = unsafe {
                let ptr = (syms.last_error)();
                if ptr.is_null() {
                    "unknown error".to_string()
                } else {
                    std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
                }
            };
            return Err(SpeechToTextError::LoadModel(msg));
        }

        let handle = HandlePtr(raw_handle);
        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<SttMsg>();

        let join_handle = std::thread::spawn(move || {
            while let Ok(msg) = msg_rx.recv() {
                call_transcribe(syms, handle.ptr(), &msg.audio_path, msg.output_tx);
            }
            unsafe { (syms.destroy)(handle.ptr()) };
        });

        Ok(Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, None)),
        })
    }

    pub fn transcribe(&self, audio_path: String) -> TokenStreamAsync {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(SttMsg {
            audio_path,
            output_tx: tx,
        });
        TokenStreamAsync::new(rx)
    }
}

// -- FFI callbacks + transcription call --

extern "C" fn segment_cb(text: *const c_char, len: usize, ud: *mut c_void) {
    let tx = unsafe { &*(ud as *const tokio::sync::mpsc::UnboundedSender<WriteOutput>) };
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len) };
    if let Ok(s) = std::str::from_utf8(bytes) {
        let _ = tx.send(WriteOutput::Token(s.to_string()));
    }
}

extern "C" fn done_cb(text: *const c_char, len: usize, ud: *mut c_void) {
    let tx = unsafe { &*(ud as *const tokio::sync::mpsc::UnboundedSender<WriteOutput>) };
    let bytes = unsafe { std::slice::from_raw_parts(text as *const u8, len) };
    let s = std::str::from_utf8(bytes).unwrap_or("").to_string();
    let _ = tx.send(WriteOutput::Done(s));
}

fn call_transcribe(
    syms: &'static SttSyms,
    handle: *mut c_void,
    audio_path: &str,
    output_tx: tokio::sync::mpsc::UnboundedSender<WriteOutput>,
) {
    let audio_cstr = match CString::new(audio_path) {
        Ok(s) => s,
        Err(e) => {
            error!(error = %e, "Invalid audio path");
            return;
        }
    };

    // SAFETY: output_tx lives on this stack frame for the entire blocking call.
    // Callbacks only fire during stt_module_transcribe, so the pointer is valid.
    let userdata = &output_tx as *const _ as *mut c_void;

    let ret = unsafe {
        (syms.transcribe)(
            handle,
            audio_cstr.as_ptr(),
            Some(segment_cb),
            Some(done_cb),
            userdata,
        )
    };

    if ret != 0 {
        let msg = unsafe {
            let ptr = (syms.last_error)();
            if ptr.is_null() {
                "unknown transcription error".to_string()
            } else {
                std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
            }
        };
        error!(error = %msg, "Transcription failed");
    }
}
