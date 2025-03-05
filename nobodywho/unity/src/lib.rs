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