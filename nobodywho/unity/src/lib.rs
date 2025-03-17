use std::ffi::{CStr, c_void, c_char};
use nobodywho::core::llm;
use std::sync::mpsc;
use std::thread;
use nobodywho::core::sampler_config::SamplerConfig;

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

#[no_mangle]
pub extern "C" fn create_chat_worker(
    model: *mut c_void,
    system_prompt: *const c_char
) -> *mut c_void {
    let (prompt_tx, prompt_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();
    
    // Start worker thread
    thread::spawn(move || {
        llm::run_completion_worker(
            model,
            prompt_rx,
            completion_tx,
            SamplerConfig::default(),
            4096, // Default context length
            system_prompt.to_string(),
            vec![], // No stop tokens for now
        );
    });

    // Return channels wrapped in a context
    Box::into_raw(Box::new(ChatContext {
        prompt_tx,
        completion_rx,
    }))
}

#[no_mangle]
pub extern "C" fn poll_responses(
    context: *mut c_void,
    on_token: extern fn(*const c_char),
    on_complete: extern fn(*const c_char),
    on_error: extern fn(*const c_char)
) {
    let context = unsafe { &*(context as *const ChatContext) };
    
    while let Ok(output) = context.completion_rx.try_recv() {
        match output {
            LLMOutput::Token(token) => on_token(to_cstr(token)),
            LLMOutput::Done(response) => on_complete(to_cstr(response)),
            LLMOutput::FatalErr(msg) => on_error(to_cstr(msg)),
        }
    }
}
