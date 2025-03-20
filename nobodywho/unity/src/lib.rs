use std::ffi::{CStr, c_void, c_char};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::ffi::CString;

use nobodywho::core::llm;
use nobodywho::core::sampler_config::SamplerConfig;
use nobodywho::llm::LLMOutput;

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
            Arc::into_raw(model) as *mut c_void
        }
        Err(err) => {
            copy_to_error_buf(error_buf, &err.to_string());
            std::ptr::null_mut()
        }
    }
}


struct ChatContext {
    prompt_tx: mpsc::Sender<String>,
    completion_rx: mpsc::Receiver<LLMOutput>,
}

#[no_mangle]
pub extern "C" fn create_chat_worker(
    model_ptr: *mut c_void,
    system_prompt: *const c_char
) -> *mut c_void {
    let model = unsafe { 
        *Box::from_raw(model_ptr as *mut llm::Model)
    };

    let system_prompt = unsafe {
        CStr::from_ptr(system_prompt)
            .to_string_lossy()
            .into_owned()
    };

    let (prompt_tx, prompt_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();
    
    thread::spawn(move || {
        llm::run_completion_worker(
            model,
            prompt_rx,
            completion_tx,
            SamplerConfig::default(),
            4096,
            system_prompt,
            vec![],
        );
    });

    Box::into_raw(Box::new(ChatContext {
        prompt_tx,
        completion_rx,
    })) as *mut c_void
}

/// Polls for upadtes to the queue of responses from the LLM
/// if any updates are available, it will call the appropriate callback
/// with the updated response or error message
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
            LLMOutput::Token(token) => {
                if let Ok(c_str) = CString::new(token) {
                    on_token(c_str.as_ptr())
                }
            },
            LLMOutput::Done(response) => {
                if let Ok(c_str) = CString::new(response) {
                    on_complete(c_str.as_ptr())
                }
            },
            LLMOutput::FatalErr(msg) => {
                if let Ok(c_str) = CString::new(msg.to_string()) {
                    on_error(c_str.as_ptr())
                }
            },
        }
    }
}


#[no_mangle]
pub extern "C" fn send_prompt(
    context: *mut c_void,
    prompt: *const c_char,
    error_buf: *mut c_char
) -> bool {
    if context.is_null() || prompt.is_null() || error_buf.is_null() {
        if !error_buf.is_null() {
            copy_to_error_buf(error_buf, "Null pointer provided");
        }
        return false;
    }

    let context = unsafe { &*(context as *const ChatContext) };
    let prompt_str = match unsafe { CStr::from_ptr(prompt).to_str() } {
        Ok(s) => s.to_owned(),
        Err(_) => {
            copy_to_error_buf(error_buf, "Invalid UTF-8 in prompt");
            return false;
        }
    };

    match context.prompt_tx.send(prompt_str) {
        Ok(_) => true,
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Failed to send prompt: {}", e));
            false
        }
    }
}

/// Dropping the context will kill the worker thread. This will force the retriever rx to error out -
/// much like we do in godot - and panic (but we dont care too much as we have purposefully killed the thread).
#[no_mangle]
pub extern "C" fn destroy_chat_worker(context: *mut c_void) {
    unsafe {
        drop(Box::from_raw(context as *mut ChatContext));
    }
}

// Converts the raw pointer back to an Arc, decreasing the reference count
// when it goes out of scope. This must be called exactly once for each
// pointer created with Arc::into_raw to prevent memory leaks.
#[no_mangle]
pub extern "C" fn destroy_model(model: *mut c_void) {
    unsafe {
        let _: Arc<llm::Model> = Arc::from_raw(model as *const llm::Model); 
    }
}
