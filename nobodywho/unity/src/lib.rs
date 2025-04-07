use std::ffi::CString;
use std::ffi::{c_char, c_void, CStr};
use std::sync::mpsc;
use std::thread;

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
        std::ptr::copy_nonoverlapping(safe_message.as_ptr() as *const c_char, error_buf, length);
        *error_buf.add(length) = 0;
    }
}

/////////////////////  MODEL  /////////////////////

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
pub extern "C" fn get_model(
    ptr: *mut c_void,
    path: *const c_char,
    use_gpu: bool,
    error_buf: *mut c_char,
) -> *mut c_void {
    let model_object = match unsafe { (ptr as *mut ModelObject).as_ref() } {
        Some(this) => this.clone(),
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

#[no_mangle]
pub extern "C" fn destroy_model(model: *mut c_void) {
    unsafe {
        drop(Box::from_raw(model as *mut ModelObject));
    }
}

/////////////////////  EMBEDDING  /////////////////////

struct EmbeddingContext {
    text_tx: mpsc::Sender<String>,
    embedding_rx: mpsc::Receiver<llm::EmbeddingsOutput>,
}

#[no_mangle]
pub extern "C" fn create_embedding_worker(
    model_ptr: *mut c_void,
    error_buf: *mut c_char,
) -> *mut c_void {
    if model_ptr.is_null() {
        copy_to_error_buf(error_buf, "Model pointer is null");
        return std::ptr::null_mut();
    }

    let model = unsafe { &mut *(model_ptr as *mut ModelObject) };
    let (text_tx, text_rx) = mpsc::channel();
    let (embedding_tx, embedding_rx) = mpsc::channel();

    thread::spawn(move || {
        llm::run_embedding_worker(model.model.clone(), text_rx, embedding_tx);
    });

    let context = Box::new(EmbeddingContext {
        text_tx,
        embedding_rx,
    });

    Box::into_raw(context) as *mut c_void
}

#[no_mangle]
pub extern "C" fn embed_text(context: *mut c_void, text: *const c_char, error_buf: *mut c_char) {
    let embedding_context = unsafe { &mut *(context as *mut EmbeddingContext) };

    let text_str: String = match unsafe { CStr::from_ptr(text).to_str() } {
        Ok(text_string) => text_string.to_owned(),
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Invalid UTF-8 in text: {}", e));
            return;
        }
    };

    match embedding_context.text_tx.send(text_str) {
        Ok(_) => {
            println!("[DEBUG] embed_text - Text sent successfully");
        }
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Failed to send text: {}", e));
            return;
        }
    }
}

#[no_mangle]
pub extern "C" fn poll_embeddings(
    context: *mut c_void,
    on_embedding: extern "C" fn(*mut f32, i32),
    on_error: extern "C" fn(*const c_char),
) {
    if context.is_null() {
        println!("[ERROR] poll_embeddings - Null context pointer received");
        return;
    }

    let embedding_context = unsafe { &mut *(context as *mut EmbeddingContext) };

    while let Ok(output) = &embedding_context.embedding_rx.try_recv() {
        match output {
            llm::EmbeddingsOutput::Embedding(embedding) => {
                let ptr = embedding.as_ptr();
                let len = embedding.len() as i32;
                on_embedding(ptr as *mut f32, len);
            }
            llm::EmbeddingsOutput::FatalError(msg) => {
                if let Ok(c_str) = CString::new(msg.to_string()) {
                    on_error(c_str.as_ptr())
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn destroy_embedding_worker(context: *mut c_void) {
    unsafe {
        drop(Box::from_raw(context as *mut EmbeddingContext));
    }
}


// TODO: just have a pointer to a struct that has the actual vec inside it. that means that the end use does
// not have to deal with length of the vec. What are the use cases where the end user wants to analyze what is inside the embeddings?
#[no_mangle]
pub extern "C" fn cosine_similarity(
    a: *const f32,
    length_a: i32,
    b: *const f32,
    length_b: i32,
) -> f32 {
    // slice it to not take ownership.
    let a_slice = unsafe { std::slice::from_raw_parts(a, length_a as usize) };
    let b_slice = unsafe { std::slice::from_raw_parts(b, length_b as usize) };
    llm::cosine_similarity(a_slice, b_slice)
}

/////////////////////  CHAT  /////////////////////

struct ChatContext {
    prompt_tx: mpsc::Sender<String>,
    completion_rx: mpsc::Receiver<LLMOutput>,
}

#[no_mangle]
pub extern "C" fn create_chat_worker(
    model_ptr: *mut c_void,
    system_prompt: *const c_char,
    stop_words: *const c_char,
    context_length: u32,
    use_grammar: bool,
    grammar: *const c_char,
    error_buf: *mut c_char,
) -> *mut c_void {
    let model = unsafe { &mut *(model_ptr as *mut ModelObject) };

    let grammar_str = unsafe {
        if grammar.is_null() {
            String::new()
        } else {
            match CStr::from_ptr(grammar).to_str() {
                Ok(s) => s.to_owned(),
                Err(_) => {
                    copy_to_error_buf(error_buf, "Invalid UTF-8 in grammar");
                    return std::ptr::null_mut();
                }
            }
        }
    };

    let system_prompt = unsafe {
        if system_prompt.is_null() {
            String::new()
        } else {
            match CStr::from_ptr(system_prompt).to_str() {
                Ok(s) => s.to_owned(),
                Err(_) => {
                    copy_to_error_buf(error_buf, "Invalid UTF-8 in system prompt");
                    return std::ptr::null_mut();
                }
            }
        }
    };

    let stop_words_vec = unsafe {
        if stop_words.is_null() {
            Vec::new()
        } else {
            match CStr::from_ptr(stop_words).to_str() {
                Ok(stop_words) => {
                    if stop_words.is_empty() {
                        Vec::new()
                    } else {
                        let stop_words_vec = stop_words
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .collect();
                        stop_words_vec
                    }
                }
                Err(_) => {
                    copy_to_error_buf(error_buf, "Invalid UTF-8 in stop words");
                    return std::ptr::null_mut();
                }
            }
        }
    };

    let mut sampler_config = SamplerConfig::default();
    sampler_config.use_grammar = use_grammar;
    if !grammar_str.is_empty() {
        sampler_config.gbnf_grammar = grammar_str;
    }

    let (prompt_tx, prompt_rx) = mpsc::channel();
    let (completion_tx, completion_rx) = mpsc::channel();

    thread::spawn(move || {
        llm::run_completion_worker(
            model.model.clone(),
            prompt_rx,
            completion_tx,
            sampler_config,
            context_length,
            system_prompt,
            stop_words_vec,
        );
    });

    let context = Box::new(ChatContext {
        prompt_tx,
        completion_rx,
    });

    let raw_ptr = Box::into_raw(context) as *mut c_void;

    raw_ptr
}

/// Polls for updates to the queue of responses from the LLM
/// if any updates are available, it will call the appropriate callback
/// with the updated response or error message
#[no_mangle]
pub extern "C" fn poll_responses(
    context: *mut c_void,
    on_token: extern "C" fn(*const c_char),
    on_complete: extern "C" fn(*const c_char),
    on_error: extern "C" fn(*const c_char),
) {
    if context.is_null() {
        println!("[ERROR] poll_responses - Null context pointer received");
        return;
    }

    let chat_context = unsafe { &mut *(context as *mut ChatContext) };

    while let Ok(output) = &chat_context.completion_rx.try_recv() {
        match output {
            LLMOutput::Token(token) => {
                if let Ok(c_str) = CString::new(token.as_str()) {
                    on_token(c_str.as_ptr())
                }
            }
            LLMOutput::Done(response) => {
                if let Ok(c_str) = CString::new(response.as_str()) {
                    on_complete(c_str.as_ptr())
                }
            }
            LLMOutput::FatalErr(msg) => {
                if let Ok(c_str) = CString::new(msg.to_string()) {
                    on_error(c_str.as_ptr())
                }
            }
        }
    }
}

#[no_mangle]
pub extern "C" fn send_prompt(context: *mut c_void, prompt: *const c_char, error_buf: *mut c_char) {
    let chat_context = unsafe { &mut *(context as *mut ChatContext) };

    let prompt_str: String = match unsafe { CStr::from_ptr(prompt).to_str() } {
        Ok(prompt_string) => prompt_string.to_owned(),
        Err(e) => {
            copy_to_error_buf(error_buf, &format!("Invalid UTF-8 in prompt: {}", e));
            return;
        }
    };

    match chat_context.prompt_tx.send(prompt_str) {
        Ok(_) => {
            println!("[DEBUG] send_prompt - Prompt sent successfully");
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    static mut RECEIVED_COMPLETE: bool = false;
    static mut RESPONSE: Option<String> = None;
    static mut EMBEDDING: Option<Vec<f32>> = None;
    extern "C" fn _on_error(error: *const c_char) {
        if let Ok(error_str) = unsafe { CStr::from_ptr(error) }.to_str() {
            println!("[ERROR] on_error - Error: {}", error_str);
        }
    }

    extern "C" fn _on_token(token: *const c_char) {
        if let Ok(token_str) = unsafe { CStr::from_ptr(token) }.to_str() {
            unsafe {
                RESPONSE = Some(match &RESPONSE {
                    Some(existing) => existing.clone() + token_str,
                    None => token_str.to_owned(),
                });
            }
        }
    }

    extern "C" fn _on_embedding(embedding: *mut f32, length: i32) {
        unsafe {
            let embedding_slice = std::slice::from_raw_parts(embedding, length as usize);
            EMBEDDING = Some(embedding_slice.to_vec());
        } 
    }

    extern "C" fn _on_complete(response: *const c_char) {
        if let Ok(response_str) = unsafe { CStr::from_ptr(response) }.to_str() {
            println!("[DEBUG] test_on_complete - Response: {}", response_str);
            unsafe {
                RECEIVED_COMPLETE = true;
            }
        }
    }

    #[test]
    fn test_create_chat_worker_with_stop_tokens() {
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;

        let model_path = CString::new("qwen2.5-1.5b-instruct-q4_0.gguf").unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);

        let system_prompt =
            CString::new("You must always list the animals in alphabetical order").unwrap();
        let stop_tokens = CString::new("fly").unwrap();
        let context_length: u32 = 4096;

        let chat_context: *mut c_void = create_chat_worker(
            model,
            system_prompt.as_ptr(),
            stop_tokens.as_ptr(),
            context_length,
            false,
            std::ptr::null(),
            error_ptr,
        );

        let prompt =
            CString::new("List these animals in alphabetical order: cat, dog, fly, lion, mouse")
                .unwrap();
        send_prompt(chat_context, prompt.as_ptr(), error_ptr);

        unsafe {
            RECEIVED_COMPLETE = false;
        }
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(15);

        while unsafe { !RECEIVED_COMPLETE } {
            if std::time::Instant::now() > timeout {
                panic!("Timed out waiting for response");
            }
            poll_responses(
                chat_context as *mut c_void,
                _on_token,
                _on_complete,
                _on_error,
            );
        }

        let response = unsafe { RESPONSE.clone().unwrap() };
        assert!(response.contains("dog"));
        assert!(response.contains("fly"));
        assert!(!response.contains("lion"));
        assert!(!response.contains("mouse"));

        destroy_chat_worker(chat_context as *mut c_void);
        destroy_model(model as *mut c_void);
        unsafe {
            RESPONSE = None;
        }
    }

    #[test]
    fn test_create_chat_worker() {
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;

        let model_path = CString::new("qwen2.5-1.5b-instruct-q4_0.gguf").unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);
        assert_eq!(
            unsafe { CStr::from_ptr(error_ptr).to_bytes() },
            &[0u8; 0],
            "Model should be loaded successfully"
        );

        let system_prompt = CString::new("You are a test assistant").unwrap();
        let context_length: u32 = 4096;

        let chat_context: *mut c_void = create_chat_worker(
            model.clone(),
            system_prompt.as_ptr(),
            std::ptr::null(),
            context_length,
            false,
            std::ptr::null(),
            error_ptr,
        );

        assert_eq!(
            unsafe { CStr::from_ptr(error_ptr).to_bytes() },
            &[0u8; 0],
            "Chat worker should be created successfully"
        );

        let prompt = CString::new("Hello, how are you?").unwrap();
        send_prompt(chat_context, prompt.as_ptr(), error_ptr);

        unsafe {
            RECEIVED_COMPLETE = false;
        }
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while unsafe { !RECEIVED_COMPLETE } {
            if std::time::Instant::now() > timeout {
                panic!("Timed out waiting for response");
            }
            poll_responses(chat_context, _on_token, _on_complete, _on_error);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            unsafe { RECEIVED_COMPLETE },
            "Should have received completion signal"
        );

        destroy_chat_worker(chat_context as *mut c_void);
        destroy_model(model as *mut c_void);
        unsafe {
            RESPONSE = None;
        }
    }

    #[test]
    fn test_create_embedding_worker() {
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;

        let model_path = CString::new("bge-small-en-v1.5-q8_0.gguf").unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);

        let embedding_context = create_embedding_worker(model, error_ptr);

        assert!(
            !embedding_context.is_null(),
            "Embedding context should not be null"
        );

        // OBS: Thread spawning is asynchronous and may not happen immediately,
        // this is why we need to sleep for a short period of time to ensure the thread is spawned. otherwise we will destroy the modelobjewct
        // and the thread will try to spawn with a null model leading to a segfault.
        // This should be a testtime issue only as the scope will be exited when crossing the language boundary allowing for the thread to be spawned... i think.
        std::thread::sleep(std::time::Duration::from_millis(1));

        destroy_embedding_worker(embedding_context);
        destroy_model(model as *mut c_void);
    }

    #[test]
    fn test_embed_text() {
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;

        let model_path = CString::new("bge-small-en-v1.5-q8_0.gguf").unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);

        let embedding_context = create_embedding_worker(model, error_ptr);

        let text = CString::new("Hello, world!").unwrap();
        embed_text(embedding_context, text.as_ptr(), error_ptr);

        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while unsafe { EMBEDDING.is_none() } {
            if std::time::Instant::now() > timeout {
                panic!("Timed out waiting for embedding");
            }
            poll_embeddings(embedding_context, _on_embedding, _on_error);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        let embedding = unsafe { EMBEDDING.take().unwrap() };
        assert!(embedding.len() > 0, "Embedding should not be empty");

        destroy_embedding_worker(embedding_context);
        destroy_model(model as *mut c_void);
    }

    #[test]
    fn test_embedding_similarity() {
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;

        let model_path = CString::new("bge-small-en-v1.5-q8_0.gguf").unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);
        let embedding_context = create_embedding_worker(model, error_ptr);

        let texts = [
            "The dragon is on the hill.",
            "The dragon is hungry for humans.",
            "This does not matter.",
        ];

        let mut embeddings = Vec::new();

        for text in texts {
            let text_cstring = CString::new(text).unwrap();
            embed_text(embedding_context, text_cstring.as_ptr(), error_ptr);

            let timeout = std::time::Instant::now() + std::time::Duration::from_secs(15);
            unsafe {
                EMBEDDING = None;
            }

            while unsafe { EMBEDDING.is_none() } {
                if std::time::Instant::now() > timeout {
                    panic!("Timed out waiting for embedding");
                }
                poll_embeddings(embedding_context, _on_embedding, _on_error);
                std::thread::sleep(std::time::Duration::from_millis(10));
            }

            let embedding = unsafe { EMBEDDING.take().unwrap() };
            embeddings.push(embedding);
        }

        let low_similarity = cosine_similarity(
            embeddings[2].as_ptr(),
            embeddings[2].len() as i32,
            embeddings[0].as_ptr(),
            embeddings[0].len() as i32,
        );
        let high_similarity = cosine_similarity(
            embeddings[0].as_ptr(),
            embeddings[0].len() as i32,
            embeddings[1].as_ptr(),
            embeddings[1].len() as i32,
        );

        assert!(
            low_similarity < high_similarity,
            "Expected similarity between low similarity ({}) to be lower than high similarity ({})",
            low_similarity,
            high_similarity
        );

        destroy_embedding_worker(embedding_context);
        destroy_model(model as *mut c_void);
    }
}
