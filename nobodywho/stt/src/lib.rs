mod audio;
mod transcribe;

use std::cell::RefCell;
use std::ffi::{c_char, c_void, CStr, CString};
use std::sync::OnceLock;
use transcribe::ModuleConfig;
use whisper_rs::{WhisperContext, WhisperContextParameters};

/// ABI version — must match the constant in nobodywho core.
pub const STT_MODULE_ABI_VERSION: u32 = 1;

// -- Logging init (once per process) --

static LOGGING_INIT: OnceLock<()> = OnceLock::new();

fn ensure_logging() {
    LOGGING_INIT.get_or_init(|| {
        whisper_rs::install_logging_hooks();
    });
}

// -- Thread-local last error --

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = RefCell::new(None);
}

fn set_last_error(msg: &str) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(msg).ok();
    });
}

// -- Handle type --

pub struct WhisperHandle {
    // WhisperState was created from WhisperContext and requires it to remain alive.
    // The field is not read directly but kept for drop ordering.
    #[allow(dead_code)]
    ctx: Box<WhisperContext>,
    state: whisper_rs::WhisperState,
    config: ModuleConfig,
}

// -- Exported FFI surface --

/// Returns the ABI version of this module. Core checks this at load time.
#[no_mangle]
pub extern "C" fn stt_module_version() -> u32 {
    STT_MODULE_ABI_VERSION
}

/// Create a new STT handle. Returns null on failure; call `stt_module_last_error` for details.
///
/// # Safety
/// `model_path` must be a valid, non-null C string. `language` and `initial_prompt` may be null.
#[no_mangle]
pub unsafe extern "C" fn stt_module_create(
    model_path: *const c_char,
    language: *const c_char,
    translate: bool,
    initial_prompt: *const c_char,
) -> *mut WhisperHandle {
    ensure_logging();

    let model_path = match CStr::from_ptr(model_path).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("Invalid model_path string: {}", e));
            return std::ptr::null_mut();
        }
    };

    let ctx = match WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
    {
        Ok(c) => Box::new(c),
        Err(e) => {
            set_last_error(&format!("Failed to load STT model: {}", e));
            return std::ptr::null_mut();
        }
    };

    let state = match ctx.create_state() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("Failed to create STT state: {}", e));
            return std::ptr::null_mut();
        }
    };

    let config = ModuleConfig::from_c(language, translate, initial_prompt);

    let handle = Box::new(WhisperHandle { ctx, state, config });
    Box::into_raw(handle)
}

/// Destroy an STT handle previously created with `stt_module_create`.
///
/// # Safety
/// `h` must be a valid pointer returned by `stt_module_create` and must not be used after this call.
#[no_mangle]
pub unsafe extern "C" fn stt_module_destroy(h: *mut WhisperHandle) {
    if !h.is_null() {
        drop(Box::from_raw(h));
    }
}

/// Transcribe an audio file. Returns 0 on success, -1 on error.
///
/// `segment_cb` is called once per segment as it is produced.
/// `done_cb` is called once with the complete transcript after all segments.
/// Both callbacks receive a pointer + length to a UTF-8 string (not null-terminated)
/// and the `userdata` pointer. The string is only valid for the duration of the callback.
///
/// Both `segment_cb` and `done_cb` may be null.
///
/// # Safety
/// `h` and `audio_path` must be valid non-null pointers. `userdata` is passed through as-is.
#[no_mangle]
pub unsafe extern "C" fn stt_module_transcribe(
    h: *mut WhisperHandle,
    audio_path: *const c_char,
    segment_cb: Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
    done_cb: Option<extern "C" fn(*const c_char, usize, *mut c_void)>,
    userdata: *mut c_void,
) -> i32 {
    let handle = &mut *h;

    let audio_path = match CStr::from_ptr(audio_path).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_last_error(&format!("Invalid audio_path string: {}", e));
            return -1;
        }
    };

    match transcribe::transcribe_streaming(
        &mut handle.state,
        &handle.config,
        audio_path,
        segment_cb,
        done_cb,
        userdata,
    ) {
        Ok(()) => 0,
        Err(msg) => {
            set_last_error(&msg);
            -1
        }
    }
}

/// Returns the last error message as a null-terminated C string, or null if there is no error.
/// The returned pointer is valid until the next call to any stt_module_* function on this thread.
#[no_mangle]
pub extern "C" fn stt_module_last_error() -> *const c_char {
    LAST_ERROR.with(|e| match &*e.borrow() {
        Some(s) => s.as_ptr(),
        None => std::ptr::null(),
    })
}
