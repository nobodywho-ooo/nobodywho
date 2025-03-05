use std::ffi::{CStr, CString, c_void, c_char};
use nobodywho::core::llm;

#[repr(C)]
#[derive(Debug)]
pub enum ModelErrorType {
    ModelNotFound = 1,
    InvalidModel = 2,
}

#[repr(C)]
pub struct ModelResult {
    handle: *mut c_void,
    success: bool,
    error_type: ModelErrorType,
    error_message: *mut c_char,
}

// FFI function for Unity integration
#[no_mangle]
pub extern "C" fn get_model(path: *const c_char, use_gpu: bool) -> ModelResult {
    // Safely convert C string to Rust string
    let path_str = unsafe {
        if path.is_null() {
            return ModelResult {
                handle: std::ptr::null_mut(),
                success: false,
                error_type: ModelErrorType::ModelNotFound,
                error_message: CString::new("Null path provided").unwrap().into_raw(),
            };
        }
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => return ModelResult {
                handle: std::ptr::null_mut(),
                success: false,
                error_type: ModelErrorType::ModelNotFound,
                error_message: CString::new("Invalid UTF-8 in path").unwrap().into_raw(),
            },
        }
    };
    
    // Call the actual LLM function
    match llm::get_model(path_str, use_gpu) {
        Ok(model) => {
            // Box the model and convert to raw pointer
            let boxed_model = Box::new(model);
            ModelResult {
                handle: Box::into_raw(boxed_model) as *mut c_void,
                success: true,
                error_type: ModelErrorType::ModelNotFound,
                error_message: std::ptr::null_mut(),
            }
        }
        Err(err) => {
            let (error_type, message) = match err {
                llm::LoadModelError::ModelNotFound(msg) => (
                    ModelErrorType::ModelNotFound,
                    msg
                ),
                llm::LoadModelError::InvalidModel(msg) => (
                    ModelErrorType::InvalidModel,
                    msg
                ),
            };
            
            ModelResult {
                handle: std::ptr::null_mut(),
                success: false,
                error_type,
                error_message: CString::new(message).unwrap().into_raw(),
            }
        }
    }
}
