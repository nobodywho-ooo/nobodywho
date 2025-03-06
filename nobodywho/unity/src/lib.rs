use std::ffi::{CStr, CString, c_void, c_char};
use nobodywho::core::llm;

// FFI function for Unity integration
#[no_mangle]
pub extern "C" fn get_model(path: *const c_char, use_gpu: bool) -> *mut c_void {
    // Safely convert C string to Rust string
    let path_str = unsafe {
        if path.is_null() {
            let error = CString::new("Null path provided").unwrap();
            return error.into_raw() as *mut c_void;
        }
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                let error = CString::new("Invalid UTF-8 in path").unwrap();
                return error.into_raw() as *mut c_void;
            }
        }
    };
    
    // Call the actual LLM function
    match llm::get_model(path_str, use_gpu) {
        Ok(model) => {
            // Box the model and convert to raw pointer
            let boxed_model = Box::new(model);
            Box::into_raw(boxed_model) as *mut c_void
        }
        Err(err) => {
            let message = match err {
                llm::LoadModelError::ModelNotFound(msg) => msg,
                llm::LoadModelError::InvalidModel(msg) => msg,
            };
            
            CString::new(message).unwrap().into_raw() as *mut c_void
        }
    }
}
