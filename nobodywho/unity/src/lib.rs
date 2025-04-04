use std::ffi::{CStr, c_void, c_char};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::ffi::CString;

use nobodywho::core::llm;
use nobodywho::core::sampler_config::SamplerConfig;
use nobodywho::llm::LLMOutput;

fn copy_to_error_buf(error_buf: *mut c_char, message: &str) {
    
    // If you change this value, you must also change the size of the error_buf in the following files:
    // NobodyWhoChat.cs, NobodyWhoModel.cs
    // always remove 1 from this value to leave room for the null terminator
    const MAX_ERROR_LENGTH: usize = 2047; 
    let length = std::cmp::min(message.len(), MAX_ERROR_LENGTH);
    let safe_message = &message[..length];
    unsafe {
        std::ptr::copy_nonoverlapping(
            safe_message.as_ptr() as *const c_char,
            error_buf, 
            length
        );
        *error_buf.add(length) = 0;
    }
}

#[derive(Clone)]
struct ModelObject {
    model: llm::Model,
}

impl ModelObject {
    pub fn new(path: &str, use_gpu: bool) -> Result<Self, String> {
        let model = match llm::get_model(path, use_gpu) {
            Ok(model) => model,
            Err(e) => {
                return Err(e.to_string());
            }
        };
        Ok(Self { model: model })
    }

}

#[no_mangle]
pub extern "C" fn get_model(ptr: *mut c_void, path: *const c_char, use_gpu: bool, error_buf: *mut c_char) -> *mut c_void {
    let model_object = match unsafe { (ptr as *mut ModelObject).as_ref() } {
        Some(this) => {
            this.clone()
        }
        None => {
            let path_str = unsafe {
                match CStr::from_ptr(path).to_str() {
                    Ok(string) => string,
                    Err(_) => {
                        copy_to_error_buf(error_buf, "Invalid UTF-8 in path");
                        return std::ptr::null_mut();
                    }
                }
            };
            
            let model_object = match ModelObject::new(path_str, use_gpu) {
                Ok(model_object) => model_object,
                Err(e) => {
                    copy_to_error_buf(error_buf, &e);
                    return std::ptr::null_mut();
                }
            };
            model_object
        }
    };

    Box::into_raw(Box::new(model_object)) as *mut c_void
}


struct ChatContext {
    prompt_tx: mpsc::Sender<String>,
    completion_rx: mpsc::Receiver<LLMOutput>,
}
#[no_mangle]
pub extern "C" fn create_chat_worker(
    model_ptr: *mut c_void,
    system_prompt: *const c_char,
    stop_tokens: *const c_char,
    context_length: u32,
    error_buf: *mut c_char
) -> *mut c_void {
    let model = unsafe { &mut *(model_ptr as *mut ModelObject) };

    let system_prompt = unsafe {
        if system_prompt.is_null() {
            println!("[DEBUG] create_chat_worker - No system prompt");
            String::new()
        } else {
            match CStr::from_ptr(system_prompt).to_str() {
                Ok(s) => {
                    println!("[DEBUG] create_chat_worker - System prompt parsed: {}", s);
                    s.to_owned()
                },
                Err(_) => {
                    copy_to_error_buf(error_buf, "Invalid UTF-8 in system prompt");
                    return std::ptr::null_mut();
                }
            }
        }
    };

    let stop_tokens_vec = unsafe {
        if stop_tokens.is_null() {
            println!("[DEBUG] create_chat_worker - No stop tokens");
            Vec::new()
        } else {
            match CStr::from_ptr(stop_tokens).to_str() {
                Ok(s) => {
                    println!("[DEBUG] create_chat_worker - Stop tokens parsed: {}", s);
                    if s.is_empty() {
                        Vec::new()
                    } else {
                        s.split(',').map(|s| s.trim().to_string()).collect()
                    }
                },
                Err(_) => {
                    copy_to_error_buf(error_buf, "Invalid UTF-8 in stop tokens");
                    return std::ptr::null_mut();
                }
            }
        }
    };

    println!("[DEBUG] create_chat_worker - Creating channels");
    let (prompt_tx, prompt_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();
    
    println!("[DEBUG] create_chat_worker - Spawning worker thread");
    thread::spawn(move || {
        println!("[DEBUG] Worker thread - Starting");
        llm::run_completion_worker(
            model.model.clone(),
            prompt_rx,
            completion_tx,
            SamplerConfig::default(),   
            context_length,
            system_prompt,
            stop_tokens_vec,
        );
        println!("[DEBUG] Worker thread - Exiting");
    });

    println!("[DEBUG] create_chat_worker - Creating context");
    let context = Box::new(ChatContext {
        prompt_tx,
        completion_rx,
    });
    println!("[DEBUG] create_chat_worker - Converting context to raw pointer");
    let raw_ptr = Box::into_raw(context) as *mut c_void;
    println!("[DEBUG] create_chat_worker - Returning raw pointer: {:?}", raw_ptr);
    raw_ptr
}

/// Polls for updates to the queue of responses from the LLM
/// if any updates are available, it will call the appropriate callback
/// with the updated response or error message
#[no_mangle]
pub extern "C" fn poll_responses(
    context: *mut c_void,
    on_token: extern fn(*const c_char),
    on_complete: extern fn(*const c_char),
    on_error: extern fn(*const c_char)
) {
    println!("[DEBUG] poll_responses - Start with context: {:?}", context);
    let chat_context = unsafe { &mut *(context as *mut ChatContext) };
    println!("[DEBUG] poll_responses - Context dereferenced");

    while let Ok(output) = &chat_context.completion_rx.try_recv() {        println!("[DEBUG] poll_responses - Received output");
        match output {
            LLMOutput::Token(token) => {
                println!("[DEBUG] poll_responses - Token received: {}", token);
                if let Ok(c_str) = CString::new(token.as_str()) {
                    on_token(c_str.as_ptr())
                }
            },
            LLMOutput::Done(response) => {
                println!("[DEBUG] poll_responses - Response complete: {}", response);
                if let Ok(c_str) = CString::new(response.as_str()) {
                    on_complete(c_str.as_ptr())
                }
            },
            LLMOutput::FatalErr(msg) => {
                println!("[DEBUG] poll_responses - Error: {}", msg);
                if let Ok(c_str) = CString::new(msg.to_string()) {
                    on_error(c_str.as_ptr())
                }
            },
        }
    }
    println!("[DEBUG] poll_responses - End");
}


#[no_mangle]
pub extern "C" fn send_prompt(
    context: *mut c_void,
    prompt: *const c_char,
    error_buf: *mut c_char
) {
    println!("[DEBUG] send_prompt - Start with context: {:?}", context);
    let chat_context = unsafe { &mut *(context as *mut ChatContext) };
    println!("[DEBUG] send_prompt - Context dereferenced");

    let prompt_str: String = match unsafe { CStr::from_ptr(prompt).to_str() } {
        Ok(prompt_string) => {
            println!("[DEBUG] send_prompt - Prompt parsed: {}", prompt_string);
            prompt_string.to_owned()
        },
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Invalid UTF-8 in prompt: {}", e));
            return;
        }
    };
    println!("[DEBUG] send_prompt - Sending prompt to channel");
    match chat_context.prompt_tx.send(prompt_str) {
        Ok(_) => println!("[DEBUG] send_prompt - Prompt sent successfully"),
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Failed to send prompt: {}", e));
            return;
        }
    }
}

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
        drop(Box::from_raw(model as *mut llm::Model)); 
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    static mut RECEIVED_COMPLETE: bool = false;
    
    extern "C" fn test_on_error(error: *const c_char) {
        println!("[DEBUG] test_on_error called");
        if let Ok(error_str) = unsafe { CStr::from_ptr(error) }.to_str() {
            println!("[DEBUG] Error: {}", error_str);
        }
    }
    
    extern "C" fn test_on_token(token: *const c_char) {
        println!("[DEBUG] test_on_token called");
        if let Ok(token_str) = unsafe { CStr::from_ptr(token) }.to_str() {
            println!("[DEBUG] Received token: {}", token_str);
        }
    }

    extern "C" fn test_on_complete(response: *const c_char) {
        println!("[DEBUG] test_on_complete called");
        if let Ok(response_str) = unsafe { CStr::from_ptr(response) }.to_str() {
            println!("[DEBUG] Complete response: {}", response_str);
            unsafe { RECEIVED_COMPLETE = true; }
        }
    }

    #[test]
    fn test_create_chat_worker_with_stop_tokens() {
        println!("[DEBUG] Starting test_create_chat_worker_with_stop_tokens");
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;
        
        println!("[DEBUG] Loading model");
        let model_path = CString::new("qwen2.5-1.5b-instruct-q4_0.gguf").unwrap();
        let model: *mut c_void = get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);
        println!("[DEBUG] Model loaded: {:?}", model);

        let system_prompt = CString::new("You must always list the animals in alphabetical order").unwrap();
        let stop_tokens = CString::new("fly").unwrap();
        let context_length: u32 = 4096;

        println!("[DEBUG] Creating chat worker");
        let chat_context: *mut c_void = create_chat_worker(
            model,
            system_prompt.as_ptr(),
            stop_tokens.as_ptr(),
            context_length,
            error_ptr,
        );
        println!("[DEBUG] Chat worker created: {:?}", chat_context);

        let prompt = CString::new("List these animals in alphabetical order: cat, dog, fly, lion, mouse").unwrap();
        println!("[DEBUG] Sending prompt");
        send_prompt(chat_context, prompt.as_ptr(), error_ptr);
        println!("[DEBUG] Prompt sent");

        unsafe { RECEIVED_COMPLETE = false; }        
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(15);
        println!("[DEBUG] Starting response polling");
        while unsafe { !RECEIVED_COMPLETE } {
            if std::time::Instant::now() > timeout {
                panic!("Timed out waiting for response");
            }
            println!("[DEBUG] Polling responses");
            poll_responses(
                chat_context as *mut c_void,
                test_on_token,
                test_on_complete,
                test_on_error
            );
            println!("[DEBUG] Poll complete");
        }
        println!("[DEBUG] Test complete");
        
        println!("[DEBUG] Destroying chat worker");
        destroy_chat_worker(chat_context as *mut c_void);
        println!("[DEBUG] Destroying model");
        destroy_model(model as *mut c_void);
        
    }

    #[test]
    fn test_create_chat_worker() {
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;
        
        let model_path = CString::new("qwen2.5-1.5b-instruct-q4_0.gguf").unwrap();
        let model: *mut c_void = get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);
        assert_eq!(unsafe { CStr::from_ptr(error_ptr).to_bytes() }, &[0u8; 0], "Model should be loaded successfully");

        let system_prompt = CString::new("You are a test assistant").unwrap();
        let context_length: u32 = 4096;
        
        let chat_context: *mut c_void = create_chat_worker(
            model.clone(),
            system_prompt.as_ptr(),
            std::ptr::null(),
            context_length,
            error_ptr,
        );

        assert_eq!(unsafe { CStr::from_ptr(error_ptr).to_bytes() }, &[0u8; 0], "Chat worker should be created successfully");

        let prompt = CString::new("Hello, how are you?").unwrap();
        send_prompt(chat_context, prompt.as_ptr(), error_ptr);
        
        unsafe { RECEIVED_COMPLETE = false; }        
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while unsafe { !RECEIVED_COMPLETE } {
            if std::time::Instant::now() > timeout {
                panic!("Timed out waiting for response");
            }
            poll_responses(
                chat_context,
                test_on_token,
                test_on_complete,
                test_on_error
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(unsafe { RECEIVED_COMPLETE }, "Should have received completion signal");

        destroy_chat_worker(chat_context as *mut c_void);
        destroy_model(model as *mut c_void);
    }

}



