use std::ffi::{CStr, c_void, c_char};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::ffi::CString;

use nobodywho::core::llm;
use nobodywho::core::sampler_config::SamplerConfig;
use nobodywho::llm::LLMOutput;

use llama_cpp_2::model::LlamaModel;

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
    system_prompt: *const c_char,
    error_buf: *mut c_char
) -> *mut c_void {
    let model: llm::Model = unsafe { Arc::from_raw(model_ptr as *const LlamaModel) };    
    
    let system_prompt = unsafe {
        match CStr::from_ptr(system_prompt).to_str() {
            Ok(s) => s.to_owned(),
            Err(_) => {
                copy_to_error_buf(error_buf, "Invalid UTF-8 in system prompt");
                return std::ptr::null_mut();
            }
        }
    };

    println!("DEBUG: system prompt: {}", system_prompt);

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

    Arc::into_raw(Arc::new(ChatContext {
        prompt_tx,
        completion_rx,
    })) as *mut c_void
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
    println!("DEBUG [poll_responses]: entry");
    let context_ref: Arc<ChatContext> = unsafe { Arc::from_raw(context as *const ChatContext) };
    let count = Arc::strong_count(&context_ref);
    println!("DEBUG [poll_responses]: Arc has {:?} strong references", count); 

    let chat_context: ChatContext = match Arc::into_inner(context_ref) {
        Some(c) => c,
        None => {
            panic!("ERROR: Arc is null - this is likely due to the memory being freed. reference count: {:?}", count);
        }
    };

    while let Ok(output) = &chat_context.completion_rx.try_recv() {
        println!("DEBUG [poll_responses]: output is Ok");
        match output {
            LLMOutput::Token(token) => {
                println!("DEBUG [poll_responses]: output is Token");
                if let Ok(c_str) = CString::new(token.as_str()) {
                    println!("DEBUG [poll_responses]: output is Token and c_str is Ok");
                    on_token(c_str.as_ptr())
                }
            },
            LLMOutput::Done(response) => {
                if let Ok(c_str) = CString::new(response.as_str()) {
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
    let _ = Arc::into_raw(Arc::new(chat_context)) as *mut c_void; // return ownership back to the caller 
}


#[no_mangle]
pub extern "C" fn send_prompt(
    context: *mut c_void,
    prompt: *const c_char,
    error_buf: *mut c_char
) {
    println!("DEBUG [send_prompt]: entry");
    let context_ref: Arc<ChatContext> = unsafe { Arc::from_raw(context as *const ChatContext) }; // +1
    let count = Arc::strong_count(&context_ref);
    if count != 1 {
        copy_to_error_buf(error_buf, &format!("[send_prompt]: Arc has {:?} strong references, it should have 1", count));
        return;
    }


    let chat_context: ChatContext = match Arc::try_unwrap(context_ref) {
        Ok(c) => c,
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Arc has {:?} strong references, it should have 1", count));   
            return;
        }
    };
    
    let prompt_str = match unsafe { CStr::from_ptr(prompt).to_str() } {
        Ok(prompt_string) => prompt_string.to_owned(),
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Invalid UTF-8 in prompt: {}", e));
            // currently this implicitly kills the last reference to the ChatContext
            return;
        }
    };
    match chat_context.prompt_tx.send(prompt_str) {
        Ok(_) => {},
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Failed to send prompt: {}", e));
            // currently this implicitly kills the last reference to the ChatContext
            return;
        }
    }
    let _ = Arc::into_raw(Arc::new(chat_context)) as *mut c_void; // return ownership back to the caller 
}

#[no_mangle]
pub extern "C" fn destroy_chat_worker(context: *mut c_void) {
    unsafe {
        let _: Arc<ChatContext> = Arc::from_raw(context as *mut ChatContext);
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



#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    static mut RECEIVED_COMPLETE: bool = false;
    
    extern "C" fn test_on_error(_error: *const c_char) { 
        println!("DEBUG: Received error callback!");
        panic!("Received error during polling"); 
    }
    extern "C" fn test_on_complete(_response: *const c_char) { 
        println!("DEBUG: Received completion callback!");
        unsafe { RECEIVED_COMPLETE = true; } 
    }
    extern "C" fn test_on_token(token: *const c_char) {
        if let Ok(token_str) = unsafe { CStr::from_ptr(token) }.to_str() {
            println!("DEBUG: Received token: {}", token_str);
        }
    }

    #[test]
    fn test_create_chat_worker() {
        let error_buf = [0u8; 1024];
        let error_ptr = error_buf.as_ptr() as *mut c_char;
        
        let model_path = CString::new("qwen2.5-1.5b-instruct-q4_0.gguf").unwrap();
        let model: *mut c_void = get_model(model_path.as_ptr(), true, error_ptr);
        assert_eq!(unsafe { CStr::from_ptr(error_ptr).to_bytes() }, &[0u8; 0], "Model should be loaded successfully");

        let system_prompt = CString::new("You are a test assistant").unwrap();
        let chat_context: *mut c_void = create_chat_worker(
            model,
            system_prompt.as_ptr(),
            error_ptr,
        );
        assert_eq!(unsafe { CStr::from_ptr(error_ptr).to_bytes() }, &[0u8; 0], "Chat worker should be created successfully");
        
        let prompt = CString::new("Hello, how are you?").unwrap();
        send_prompt(chat_context, prompt.as_ptr(), error_ptr);
        
        unsafe { RECEIVED_COMPLETE = false; }
        
        println!("DEBUG: Starting polling loop...");
        
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while unsafe { !RECEIVED_COMPLETE } {
            if std::time::Instant::now() > timeout {
                println!("DEBUG: Polling timed out!");
                panic!("Timed out waiting for response");
            }
            println!("DEBUG: Polling responses...");
            poll_responses(
                chat_context as *mut c_void,
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

