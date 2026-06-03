//! Strong overrides for Emscripten's weak file-syscall stubs.
//!
//! Emscripten's standalone.c defines `__syscall_openat` / `__syscall_stat64` /
//! etc. as weak symbols returning -EPERM/-ENOSYS. wasm-ld silently uses those
//! stubs, so libc `fopen` fails with EPERM before reaching our MEMFS.
//!
//! This module provides strong `#[no_mangle] pub extern "C"` overrides that
//! route through `Module.FS` (installed on globalThis by pre.js) so llama.cpp's
//! `fopen` calls land on the bytes we wrote into MEMFS from JS.

#![cfg(target_family = "wasm")]

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

type SizeT = usize;

// errno constants — values from Emscripten's musl-derived errno table.
// Used as -errno return codes per the syscall ABI.
const EBADF: c_int = 8;
const EIO: c_int = 29;

/// Resolve `Module.FS` via `js_sys::Reflect`; returns EIO on failure
/// (can't panic from inside an EH-disabled C++ callstack).
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

/// Look up and call a method on the FS namespace.
fn fs_call(fs: &JsValue, method: &str, args: &[JsValue]) -> Result<JsValue, c_int> {
    let func_val = js_sys::Reflect::get(fs, &JsValue::from_str(method)).map_err(|_| EIO)?;
    let func: js_sys::Function = func_val.dyn_ref::<js_sys::Function>().ok_or(EIO)?.clone();
    let args_array = js_sys::Array::new();
    for a in args {
        args_array.push(a);
    }
    js_sys::Reflect::apply(&func, fs, &args_array).map_err(|e| {
        // Return already-negated errno; default -EIO for unknown thrown shape.
        if let Ok(errno) = js_sys::Reflect::get(&e, &"errno".into()) {
            if let Some(n) = errno.as_f64() {
                return -(n as c_int);
            }
        }
        -EIO
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

/// Stat `path_str` via FS.stat and write the result into `buf` via SYSCALLS.writeStat.
fn stat_into_buf(path_str: &str, buf: *mut u8) -> c_int {
    let fs = match fs_namespace() {
        Ok(fs) => fs,
        Err(e) => return -e,
    };
    let stat_result = fs_call(&fs, "stat", &[JsValue::from_str(path_str)]);
    let stat_obj = match stat_result {
        Ok(o) => o,
        Err(e) => return e,
    };

    // Delegate struct stat layout to SYSCALLS.writeStat — avoids reproducing
    // Emscripten's layout in Rust and keeps it from drifting.
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
    stat_into_buf(path_str, buf)
}

/// `__syscall_fstat64` strong override — stat by fd.
#[no_mangle]
pub unsafe extern "C" fn __syscall_fstat64(fd: c_int, buf: *mut u8) -> c_int {
    let fs = match fs_namespace() {
        Ok(fs) => fs,
        Err(e) => return -e,
    };
    // Look up path via SYSCALLS.getStreamFromFD(fd).path; FS has no public fstat.
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
    stat_into_buf(&path_str, buf)
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

/// `_mmap_js` strong override — routes musl's mmap through `FS.mmap` (MEMFS + NODEFS).
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
            // ptr == 0 means FS.mmap returned a malformed result; a NULL base corrupts reads.
            if ptr == 0 {
                return -EIO;
            }
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

/// `_munmap_js` strong override — intentional no-op. Emscripten's C-side munmap
/// already frees the backing buffer; this hook is only for msync/writeback.
/// Must override anyway because the weak stub returns -ENOSYS.
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
