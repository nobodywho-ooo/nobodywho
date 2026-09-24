//! OpenCL entry points for ggml on Android, resolved by name from the device's
//! public `libOpenCL.so`.
//!
//! `android/opencl-api.h` is force-included into ggml-opencl and turns every
//! OpenCL function it calls into a pointer with the symbol `nobodywho_<name>`.
//! This module defines those pointers and fills them the first time ggml calls
//! `clGetPlatformIDs`, which it does before any other OpenCL function. Nothing
//! reads a vendor ICD dispatch table, which is what broke on Qualcomm; see
//! `android/OPENCL_LINKING.md`. Without a usable driver ggml sees no platforms
//! and falls back to Vulkan or CPU.

use std::ffi::{c_void, CStr, CString};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::OnceLock;
use tracing::{info, warn};

const CL_INVALID_OPERATION: i32 = -59;
const CL_PLATFORM_NOT_FOUND_KHR: i32 = -1001;

/// One pointer per OpenCL function, plus `TABLE` to fill them by name.
macro_rules! opencl_table {
    ($($name:ident),* $(,)?) => {
        $(
            #[allow(non_upper_case_globals)]
            #[export_name = concat!("nobodywho_", stringify!($name))]
            static $name: AtomicPtr<c_void> = AtomicPtr::new(null_mut());
        )*
        static TABLE: &[(&str, &AtomicPtr<c_void>)] = &[$((stringify!($name), &$name)),*];
    };
}
// build.rs expands android/opencl-functions.inc into one `opencl_table!` call.
include!(concat!(env!("OUT_DIR"), "/opencl_functions.rs"));

type GetPlatformIds = unsafe extern "C" fn(u32, *mut *mut c_void, *mut u32) -> i32;

/// ggml's `clGetPlatformIDs`: loads the driver once, then forwards to it.
#[export_name = "nobodywho_clGetPlatformIDs"]
unsafe extern "C" fn get_platform_ids(
    num_entries: u32,
    platforms: *mut *mut c_void,
    num_platforms: *mut u32,
) -> i32 {
    static DRIVER: OnceLock<Option<GetPlatformIds>> = OnceLock::new();
    let Some(real) = *DRIVER.get_or_init(load_driver) else {
        if !num_platforms.is_null() {
            unsafe { *num_platforms = 0 };
        }
        return CL_PLATFORM_NOT_FOUND_KHR;
    };
    unsafe { real(num_entries, platforms, num_platforms) }
}

fn load_driver() -> Option<GetPlatformIds> {
    // Never closed once used: the pointers point into it for the whole process.
    let driver =
        unsafe { libc::dlopen(c"libOpenCL.so".as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
    if driver.is_null() {
        warn!(error = %dl_error(), "OpenCL unavailable: cannot load libOpenCL.so");
        return None;
    }
    let symbol = |name: &str| {
        let name = CString::new(name).expect("OpenCL function names contain no NUL");
        unsafe { libc::dlsym(driver, name.as_ptr()) }
    };

    let get_platform_ids = symbol("clGetPlatformIDs");
    let mut missing = Vec::new();
    let mut stubbed = Vec::new();
    if get_platform_ids.is_null() {
        missing.push("clGetPlatformIDs");
    }
    let mut resolved = Vec::with_capacity(TABLE.len());
    for &(name, slot) in TABLE {
        let mut function = symbol(name);
        if function.is_null() {
            match error_stub(name) {
                Some(stub) => {
                    function = stub;
                    stubbed.push(name);
                }
                None => missing.push(name),
            }
        }
        resolved.push((slot, function));
    }
    if !missing.is_empty() {
        warn!(
            ?missing,
            "OpenCL unavailable: the driver lacks required functions"
        );
        unsafe { libc::dlclose(driver) };
        return None;
    }

    for (slot, function) in resolved {
        slot.store(function, Ordering::Release);
    }
    info!(?stubbed, "OpenCL driver loaded; functions resolved by name");
    Some(unsafe { std::mem::transmute::<*mut c_void, GetPlatformIds>(get_platform_ids) })
}

fn dl_error() -> String {
    let error = unsafe { libc::dlerror() };
    if error.is_null() {
        return "unknown error".to_owned();
    }
    unsafe { CStr::from_ptr(error) }
        .to_string_lossy()
        .into_owned()
}

/// Stand-ins for the newer functions older drivers may lack. They report an
/// error, never fake success, as the previous C shim did.
fn error_stub(name: &str) -> Option<*mut c_void> {
    let stub = match name {
        "clCreateBufferWithProperties" => create_buffer_with_properties_unavailable as *mut c_void,
        "clGetKernelSubGroupInfo" => get_kernel_sub_group_info_unavailable as *mut c_void,
        _ => return None,
    };
    Some(stub)
}

unsafe extern "C" fn create_buffer_with_properties_unavailable(
    _context: *mut c_void,
    _properties: *const u64,
    _flags: u64,
    _size: usize,
    _host_ptr: *mut c_void,
    errcode_ret: *mut i32,
) -> *mut c_void {
    if !errcode_ret.is_null() {
        unsafe { *errcode_ret = CL_INVALID_OPERATION };
    }
    null_mut()
}

#[allow(clippy::too_many_arguments)]
unsafe extern "C" fn get_kernel_sub_group_info_unavailable(
    _kernel: *mut c_void,
    _device: *mut c_void,
    _param_name: u32,
    _input_value_size: usize,
    _input_value: *const c_void,
    _param_value_size: usize,
    _param_value: *mut c_void,
    _param_value_size_ret: *mut usize,
) -> i32 {
    CL_INVALID_OPERATION
}
