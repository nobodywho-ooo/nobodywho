use std::ffi::{CStr, c_void, c_char};
use nobodywho::core::llm;

fn copy_to_error_buf(error_buf: *mut c_char, message: &str) {
    unsafe {
        std::ptr::copy_nonoverlapping(
            message.as_ptr() as *const c_char,
            error_buf,
            message.len()
        );
        *error_buf.add(message.len()) = 0;
    }
}

#[no_mangle]
pub extern "C" fn get_model(path: *const c_char, use_gpu: bool, error_buf: *mut c_char) -> *mut c_void {

    if error_buf.is_null() { return std::ptr::null_mut(); }

    let path_str = unsafe {
        match CStr::from_ptr(path).to_str() {
            Ok(s) => s,
            Err(_) => {
                copy_to_error_buf(error_buf, "Invalid UTF-8 in path");
                return std::ptr::null_mut();
            }
        }
    };
    
    match llm::get_model(path_str, use_gpu) {
        Ok(model) => {
            Box::into_raw(Box::new(model)) as *mut c_void
        }
        Err(err) => {
            copy_to_error_buf(error_buf, &err.to_string());
            std::ptr::null_mut()
        }
    }
}
