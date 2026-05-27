//! Strong overrides for Emscripten's weak file-syscall stubs.
//!
//! Emscripten's `system/lib/standalone/standalone.c` declares `__syscall_openat`
//! / `__syscall_stat64` / etc. as `weak` symbols that always return -EPERM or
//! -ENOSYS. They're meant for the standalone (no-JS-host) build mode. The
//! weak stubs get silently linked in to OUR build too — we DO have a JS host
//! with a working MEMFS, but wasm-ld is happy to satisfy `__syscall_openat`
//! references against the weak stub and never emit an import for us to
//! override.
//!
//! Result before this module: libc `fopen` → libc internals → weak
//! `__syscall_openat` → returns -EPERM → fopen returns NULL with errno=EPERM.
//! `gguf_init_from_file` logs "Operation not permitted" and abort.
//!
//! Result with this module: we define `__syscall_openat` (etc.) as STRONG
//! symbols in Rust with `#[no_mangle] pub extern "C" fn`. wasm-ld picks
//! the strong over the weak. The body calls into JS via `js_sys::Reflect`
//! against `Module.FS.open` / `FS.stat` / etc. — same FS that
//! `Module.FS.writeFile` populates from JS, so reads land on the bytes
//! we wrote there.
//!
//! For the wasm32 (single-threaded) target we don't have to worry about
//! `Module` racing or going away — it's installed on `globalThis` by
//! `pkg-bundler/pre.js` at module init and lives for the lifetime of the
//! wasm instance.

#![cfg(target_family = "wasm")]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

type SizeT = usize;

// errno constants — values from Emscripten's musl-derived errno table.
// Used as -errno return codes per the syscall ABI.
const EBADF: c_int = 8;
const ENOENT: c_int = 44;
const EIO: c_int = 29;

/// Resolve `Module.FS` once per call via `js_sys::Reflect`. Returns a
/// `JsValue` whose underlying object is the FS namespace, ready for
/// per-method lookups. Bails to a sentinel error if Module or FS
/// isn't present — that should never happen with our pre.js setup
/// but we don't want to panic from inside an EH-disabled C++ callstack.
fn fs_namespace() -> Result<JsValue, c_int> {
    let global_obj = js_sys::global();
    let module =
        js_sys::Reflect::get(&global_obj, &JsValue::from_str("Module")).map_err(|_| EIO)?;
    if module.is_undefined() || module.is_null() {
        return Err(EIO);
    }
    let fs = js_sys::Reflect::get(&module, &JsValue::from_str("FS")).map_err(|_| EIO)?;
    if fs.is_undefined() || fs.is_null() {
        return Err(EIO);
    }
    Ok(fs)
}

/// Look up a method on the FS namespace and invoke it. The trailing
/// generic is just here to keep the call sites short.
fn fs_call(fs: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, c_int> {
    let func_val = js_sys::Reflect::get(fs, &JsValue::from_str(method)).map_err(|_| EIO)?;
    let func: js_sys::Function = func_val.dyn_ref::<js_sys::Function>().ok_or(EIO)?.clone();
    let args_array = js_sys::Array::new();
    for a in args {
        args_array.push(a);
    }
    js_sys::Reflect::apply(&func, fs, &args_array).map_err(|e| {
        // FS.ErrnoError has an `errno` property. Translate to our negative
        // return value. Default to EIO for any other thrown shape.
        if let Ok(errno) = js_sys::Reflect::get(&e, &"errno".into()) {
            if let Some(n) = errno.as_f64() {
                return -(n as c_int);
            }
        }
        EIO
    })
}

/// `__syscall_openat` strong override.
///
/// libc fopen("rb") translates to openat(AT_FDCWD, path, O_RDONLY).
/// flags / mode are passed through to `Module.FS.open(path, flags, mode)`.
///
/// # Safety
///
/// `path` must be a NUL-terminated C string valid for the duration of the
/// call. Callers are libc's openat path which always passes a valid C
/// string from the user's `fopen` argument.
// `mode` is declared as `intptr_t` in Emscripten's wasm64 standalone
// libc (i64 on wasm64, i32 on wasm32). Using `isize` makes the Rust
// signature match the linker's expectation on both pointer widths.
// We narrow back to c_int when calling FS.open.
#[no_mangle]
pub unsafe extern "C" fn __syscall_openat(
    _dirfd: c_int,
    path: *const c_char,
    flags: c_int,
    mode: isize,
) -> c_int {
    let fs = match fs_namespace() {
        Ok(fs) => fs,
        Err(e) => return -e,
    };
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return -EBADF,
    };
    let result = fs_call(
        &fs,
        "open",
        &[
            JsValue::from_str(path_str),
            JsValue::from_f64(flags as f64),
            JsValue::from_f64(mode as f64),
        ],
    );
    match result {
        Ok(stream) => {
            // FS.open returns an FSStream; the fd we want is at `.fd`.
            match js_sys::Reflect::get(&stream, &JsValue::from_str("fd")) {
                Ok(fd) => fd.as_f64().map(|n| n as c_int).unwrap_or(-EIO),
                Err(_) => -EIO,
            }
        }
        Err(e) => e, // already negated
    }
}

/// Common stat helper. `kind` is either "stat" (follows symlinks) or
/// "lstat" (doesn't). Writes the stat into `buf` via SYSCALLS.writeStat
/// — which we reach by going through `Module.SYSCALLS` instead of FS.
fn stat_into_buf(path_str: &str, buf: *mut u8, lstat: bool) -> c_int {
    let fs = match fs_namespace() {
        Ok(fs) => fs,
        Err(e) => return -e,
    };
    let stat_result = fs_call(
        &fs,
        if lstat { "lstat" } else { "stat" },
        &[JsValue::from_str(path_str)],
    );
    let stat_obj = match stat_result {
        Ok(o) => o,
        Err(e) => return e,
    };

    // SYSCALLS.writeStat(buf, stat) writes into the wasm memory at `buf`
    // using the Emscripten-defined struct stat layout. We delegate
    // because reproducing the layout in Rust is error-prone and would
    // drift if Emscripten's emitted stat struct changes.
    let global_obj = js_sys::global();
    let module = match js_sys::Reflect::get(&global_obj, &"Module".into()) {
        Ok(m) => m,
        Err(_) => return -EIO,
    };
    let syscalls = match js_sys::Reflect::get(&module, &"SYSCALLS".into()) {
        Ok(s) if !s.is_undefined() && !s.is_null() => s,
        _ => return -EIO,
    };
    let write_stat_val = match js_sys::Reflect::get(&syscalls, &"writeStat".into()) {
        Ok(v) => v,
        Err(_) => return -EIO,
    };
    let write_stat = match write_stat_val.dyn_ref::<js_sys::Function>() {
        Some(f) => f.clone(),
        None => return -EIO,
    };
    match write_stat.call2(
        &syscalls,
        &JsValue::from_f64(buf as usize as f64),
        &stat_obj,
    ) {
        Ok(_) => 0,
        Err(_) => -EIO,
    }
}

/// `__syscall_stat64` strong override (follows symlinks).
///
/// # Safety
///
/// `path` is a valid NUL-terminated C string; `buf` points to writable
/// memory at least `sizeof(struct stat)` bytes large (the C caller's
/// responsibility).
#[no_mangle]
pub unsafe extern "C" fn __syscall_stat64(path: *const c_char, buf: *mut u8) -> c_int {
    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return -EBADF,
    };
    stat_into_buf(path_str, buf, false)
}

/// `__syscall_fstat64` strong override — stat by fd.
#[no_mangle]
pub unsafe extern "C" fn __syscall_fstat64(fd: c_int, buf: *mut u8) -> c_int {
    let fs = match fs_namespace() {
        Ok(fs) => fs,
        Err(e) => return -e,
    };
    // First, look up the path via SYSCALLS.getStreamFromFD(fd).path so
    // we can re-stat by path. (FS.stat takes a path; there's no public
    // FS.fstat that we can call uniformly.)
    let global_obj = js_sys::global();
    let module = match js_sys::Reflect::get(&global_obj, &"Module".into()) {
        Ok(m) => m,
        Err(_) => return -EIO,
    };
    let syscalls = match js_sys::Reflect::get(&module, &"SYSCALLS".into()) {
        Ok(s) if !s.is_undefined() && !s.is_null() => s,
        _ => return -EIO,
    };
    let get_stream_val = match js_sys::Reflect::get(&syscalls, &"getStreamFromFD".into()) {
        Ok(v) => v,
        Err(_) => return -EIO,
    };
    let get_stream = match get_stream_val.dyn_ref::<js_sys::Function>() {
        Some(f) => f.clone(),
        None => return -EIO,
    };
    let stream = match get_stream.call1(&syscalls, &JsValue::from_f64(fd as f64)) {
        Ok(s) => s,
        Err(_) => return -EBADF,
    };
    let path_val = match js_sys::Reflect::get(&stream, &"path".into()) {
        Ok(p) => p,
        Err(_) => return -EBADF,
    };
    let path_str = match path_val.as_string() {
        Some(s) => s,
        None => return -EBADF,
    };
    let _ = fs; // suppress warning if `fs` is unused on this branch
    stat_into_buf(&path_str, buf, false)
}

/// Look up a `SYSCALLS` method and invoke it.
fn syscalls_call(method: &str, args: &[JsValue]) -> Result<JsValue, c_int> {
    let global = js_sys::global();
    let module = js_sys::Reflect::get(&global, &"Module".into()).map_err(|_| EIO)?;
    let syscalls = js_sys::Reflect::get(&module, &"SYSCALLS".into()).map_err(|_| EIO)?;
    let func_val = js_sys::Reflect::get(&syscalls, &JsValue::from_str(method)).map_err(|_| EIO)?;
    let func: js_sys::Function = func_val.dyn_ref::<js_sys::Function>().ok_or(EIO)?.clone();
    let args_array = js_sys::Array::new();
    for a in args {
        args_array.push(a);
    }
    js_sys::Reflect::apply(&func, &syscalls, &args_array).map_err(|e| {
        if let Ok(errno) = js_sys::Reflect::get(&e, &"errno".into()) {
            if let Some(n) = errno.as_f64() {
                return n as c_int;
            }
        }
        EIO
    })
}

/// `_mmap_js` strong override.
///
/// Musl's mmap calls this to perform the actual mapping. The weak stub
/// in `standalone.c` returns -ENOSYS. We route through `FS.mmap` which
/// supports both MEMFS and NODEFS.
///
/// # Safety
///
/// `allocated` and `addr` must be valid writable pointers (caller's
/// responsibility — musl passes stack addresses).
#[no_mangle]
pub unsafe extern "C" fn _mmap_js(
    len: SizeT,
    prot: c_int,
    flags: c_int,
    fd: c_int,
    offset: i64,
    allocated: *mut c_int,
    addr: *mut *mut u8,
) -> c_int {
    let stream = match syscalls_call("getStreamFromFD", &[JsValue::from_f64(fd as f64)]) {
        Ok(s) => s,
        Err(e) => return -e,
    };

    let fs = match fs_namespace() {
        Ok(fs) => fs,
        Err(e) => return -e,
    };
    let mmap_result = fs_call(
        &fs,
        "mmap",
        &[
            stream,
            JsValue::from_f64(len as f64),
            JsValue::from_f64(offset as f64),
            JsValue::from_f64(prot as f64),
            JsValue::from_f64(flags as f64),
        ],
    );
    match mmap_result {
        Ok(res) => {
            let ptr_val =
                js_sys::Reflect::get(&res, &"ptr".into()).unwrap_or(JsValue::from_f64(0.0));
            let alloc_val =
                js_sys::Reflect::get(&res, &"allocated".into()).unwrap_or(JsValue::FALSE);
            let ptr = ptr_val.as_f64().unwrap_or(0.0) as usize;
            let alloc = if alloc_val.as_bool().unwrap_or(false) {
                1
            } else {
                0
            };
            *addr = ptr as *mut u8;
            *allocated = alloc;
            0
        }
        Err(e) => e, // already negated
    }
}

/// `_munmap_js` strong override.
///
/// For read-only mappings (our use case) this is a no-op. The weak stub
/// returns -ENOSYS which would make munmap fail.
#[no_mangle]
pub unsafe extern "C" fn _munmap_js(
    _addr: usize,
    _len: SizeT,
    _prot: c_int,
    _flags: c_int,
    _fd: c_int,
    _offset: i64,
) -> c_int {
    0
}
