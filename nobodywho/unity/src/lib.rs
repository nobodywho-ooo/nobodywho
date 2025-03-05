use nobodywho::core::llm;

// FFI function for Unity integration
#[no_mangle]
pub extern "C" fn get_model(path: *const std::os::raw::c_char, use_gpu: bool) -> *mut llm::Model {
    use std::ffi::CStr;
    
    // Safely convert C string to Rust string
    let path_str = unsafe {
        if path.is_null() {
            return std::ptr::null_mut();
        }
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => return std::ptr::null_mut(),
        }
    };
    
    // Call the actual LLM function
    match llm::get_model(path_str, use_gpu) {
        Ok(model) => {
            // Box the model and convert to raw pointer
            let boxed_model = Box::new(model);
            Box::into_raw(boxed_model)
        }
        Err(_) => std::ptr::null_mut()
    }
}

#[repr(C)]
pub struct ModelResult {
    handle: *mut c_void,
    success: bool,
    error_code: i32,
    error_message: *mut c_char,
}

#[no_mangle]
pub extern "C" fn get_model_ex(path: *const c_char, use_gpu: bool) -> ModelResult {
    // Error codes
    const ERROR_NONE: i32 = 0;
    const ERROR_FILE_NOT_FOUND: i32 = 1;
    const ERROR_INVALID_MODEL: i32 = 2;
    
    match std::panic::catch_unwind(|| {
        // Try to perform the operation...
        match actual_get_model(path_str, use_gpu) {
            Ok(model) => ModelResult {
                handle: Box::into_raw(Box::new(model)) as *mut c_void,
                success: true,
                error_code: ERROR_NONE,
                error_message: std::ptr::null_mut(),
            },
            Err(e) => {
                // Map Rust error to error code and message
                let (code, msg) = if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                    if io_err.kind() == std::io::ErrorKind::NotFound {
                        (ERROR_FILE_NOT_FOUND, format!("File not found: {}", path_str))
                    } else {
                        (ERROR_INVALID_MODEL, format!("IO error: {}", io_err))
                    }
                } else {
                    (ERROR_INVALID_MODEL, format!("Error: {}", e))
                };
                
                ModelResult {
                    handle: std::ptr::null_mut(),
                    success: false,
                    error_code: code,
                    error_message: CString::new(msg).unwrap().into_raw(),
                }
            }
        }
    }) {
        Ok(result) => result,
        Err(_) => ModelResult {
            handle: std::ptr::null_mut(),
            success: false,
            error_code: 999, // Panic error code
            error_message: CString::new("Panic in get_model").unwrap().into_raw(),
        },
    }
}