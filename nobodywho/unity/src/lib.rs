use nobodywho::chat;
use nobodywho::chat::ChatMsg;
use nobodywho::llm;
use nobodywho::sampler_config::SamplerConfig;
use std::ffi::CString;
use std::{
    ffi::{c_char, c_void, CStr},
    sync::{Arc, Mutex},
};

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
    embed_tx: tokio::sync::mpsc::Sender<String>,
    runtime: tokio::runtime::Runtime,
}

// Why this is Send (not technically):
// - Neither Caller or callback can be dropped on the other side of the ABI,
//   so by using this you must promise that you code on the other side is memory safe.
//   In Unity, this is done by allocing memory specifically using GChandle to avoid garbage collection.
// - As we cannot declare the data inside the pointer as const we can really guarentee - through the compiler that
//   the data will not be changed, leading to data races, howveer we have no need and should not change the data in the object from here. So please dont change stuff on this side.
// - there are no thread local variables we use for these so we are gucci in this aspect.
struct EmbeddingAdapter {
    caller: Arc<Mutex<*const c_void>>,
    callback: extern "C" fn(*const c_void, *const f32, i32),
}

unsafe impl Send for EmbeddingAdapter {}

impl chat::EmbeddingOutput for EmbeddingAdapter {
    fn emit_embedding(&self, embd: Vec<f32>) {
        while let Err(e) = self.caller.try_lock() {
            println!("[ERROR] emit_embedding - Failed to lock caller: {}", e);
        }
        let caller_ptr = self.caller.lock().unwrap();
        println!("[DEBUG] emit_embedding - Locked caller_ptr: {:?}, embedding length: {}", *caller_ptr, embd.len());
        (self.callback)(*caller_ptr, embd.as_ptr(), embd.len() as i32);
    }
}

/// Apart from the model pointer it also takes two very imnportant pointer:
/// - A pointer to a static callback function that will be called when an embedding is finished.
/// it is improtant that this callback takes a reference to the object implements the callback.
/// - A Userpointer, ie pointer to an instantaitated object, which class implements the static function.
/// the simple example without marshalling to be ffi complaint looks like this:
///
/// ```
/// public class EmbeddingObject {
///     private static void OnEmbeddingCallback(IntPtr caller, IntPtr data, int length) {
///         let this = GCHandle.FromIntPtr(caller).Target as EmbeddingObject;
///         let embedding = float_from_ptr(data, length);
///         // do stuff with the embedding
///     }
/// }
/// ```
#[no_mangle]
pub extern "C" fn create_embedding_worker(
    model_ptr: *mut c_void,
    caller_ptr: *const c_void,
    callback: extern "C" fn(*const c_void, *const f32, i32),
    error_buf: *mut c_char,
) -> *mut c_void {
    if model_ptr.is_null() {
        copy_to_error_buf(error_buf, "Model pointer is null");
        return std::ptr::null_mut();
    }

    if caller_ptr.is_null() {
        copy_to_error_buf(error_buf, "Caller pointer is null");
        return std::ptr::null_mut();
    }

    let model = unsafe { &mut *(model_ptr as *mut ModelObject) };

    let params = llm::LLMActorParams {
        model: model.model.clone(),
        sampler_config: SamplerConfig::default(),
        n_ctx: 4096,
        stop_tokens: vec![],
        use_embeddings: true,
    };

    // Add debug print for original caller_ptr
    println!("[DEBUG] create_embedding_worker - Original caller_ptr: {:?}", caller_ptr);

    let caller = Arc::new(Mutex::new(caller_ptr));

    let adapter = EmbeddingAdapter { caller, callback };

    let (embed_tx, embed_rx) = tokio::sync::mpsc::channel(4096);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .build()
        .expect("Failed to create Tokio runtime");

    runtime.spawn(async move {
        chat::simple_embedding_loop(params, embed_rx, Box::new(adapter))
            .await
            .unwrap_or_else(|e| {
                // TODO: find a way to propegate the error to c#
                println!("[ERROR] create_embedding_worker - Error: {}", e);
                ()
            });
    });
    Box::into_raw(Box::new(EmbeddingContext { embed_tx, runtime })) as *mut c_void
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


    let embed_tx = embedding_context.embed_tx.clone();
    let runtime = &embedding_context.runtime;
    // Spawn a task to send the text. Don't capture error_buf.
    runtime.spawn(async move {
        if let Err(e) = embed_tx.send(text_str).await {
            // TODO: find a way to propegate the error to c#
            println!("[ERROR] embed_text - Failed to send text to embedding worker: {}", e);
        }
    });
}

#[no_mangle]
pub extern "C" fn destroy_embedding_worker(context: *mut c_void) {
    unsafe {
        drop(Box::from_raw(context as *mut EmbeddingContext));
    }
}

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
    msg_tx: tokio::sync::mpsc::Sender<ChatMsg>,
    runtime: tokio::runtime::Runtime,
}
// Why this is Send (not technically):
// - Neither Caller or callback can be dropped on the other side of the ABI,
//   so by using this you must promise that you code on the other side is memory safe.
//   In Unity, this is done by allocing memory specifically using GChandle to avoid garbage collection.
// - As we cannot declare the data inside the pointer as const we can really guarentee - through the compiler that
//   the data will not be changed, leading to data races, howveer we have no need and should not change the data in the object from here. So please dont change stuff on this side.
// - there are no thread local variables we use for these so we are gucci in this aspect.
//
// Why this is Sync (with caveats):
// - The `extern "C" fn` pointers are inherently Sync (it's safe for multiple threads to read the same function pointer).
// - The primary concern is the `caller: Arc<Mutex<*const c_void>>`.
// - `*const c_void` is inherently not sync or send, but is only accesed through the Mutex thus making it sync - unless we change either of the methods
//    on the other side of the ABI
// - The Caveat is that the callback and caller needs to be acessible from another thread.
//   in C# this means that using a standard GHandle is not suffiucient as we are crossing AppDomains
struct ChatAdapter {
    caller: Arc<Mutex<*const c_void>>,
    token_callback: extern "C" fn(*const c_void, *const c_char),
    response_callback: extern "C" fn(*const c_void, *const c_char),
    error_callback: extern "C" fn(*const c_void, *const c_char),
}

unsafe impl Send for ChatAdapter {}

impl chat::ChatOutput for ChatAdapter {
    // Blockingly waits for the caller
    fn emit_token(&self, token: String) {
        while let Err(e) = self.caller.try_lock() {
            println!("[ERROR] emit_token - Failed to lock caller: {}", e);
        }
        let caller_ptr = self.caller.lock().unwrap();
        println!("[DEBUG] emit_token - Locked caller_ptr: {:?}, token: {}", *caller_ptr, token);
        
        // Create a CString to ensure the string stays valid
        let token_cstr = match CString::new(token) {
            Ok(cstr) => cstr,
            Err(e) => {
                println!("[ERROR] emit_token - Failed to create CString: {}", e);
                return;
            }
        };

        (self.token_callback)(*caller_ptr, token_cstr.as_ptr() as *const i8);
    }
    
    fn emit_response(&self, resp: String) {
        while let Err(e) = self.caller.try_lock() {
            println!("[ERROR] emit_response - Failed to lock caller: {}", e);
        }
        let caller_ptr = self.caller.lock().unwrap();
        println!("[DEBUG] emit_response - Locked caller_ptr: {:?}, length: {}", *caller_ptr, resp.len());
        
        // Create a CString to ensure the string stays valid
        let resp_cstr = match CString::new(resp) {
            Ok(cstr) => cstr,
            Err(e) => {
                println!("[ERROR] emit_response - Failed to create CString: {}", e);
                return;
            }
        };
        
        (self.response_callback)(*caller_ptr, resp_cstr.as_ptr());
    }
    
    fn emit_error(&self, err: String) {
        while let Err(e) = self.caller.try_lock() {
            println!("[ERROR] emit_error - Failed to lock caller: {}", e);
        }
        let caller_ptr = self.caller.lock().unwrap();
        println!("[DEBUG] emit_error - Locked caller_ptr: {:?}, error: {}", *caller_ptr, err);
        
        // Create a CString to ensure the string stays valid
        let err_cstr = match CString::new(err) {
            Ok(cstr) => cstr,
            Err(e) => {
                println!("[ERROR] emit_error - Failed to create CString: {}", e);
                return;
            }
        };

        (self.error_callback)(*caller_ptr, err_cstr.as_ptr());
    }
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
    caller_ptr: *const c_void,
    on_token: extern "C" fn(*const c_void, *const c_char),
    on_complete: extern "C" fn(*const c_void, *const c_char),
    on_error: extern "C" fn(*const c_void, *const c_char),
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

    let params = llm::LLMActorParams {
        model: model.model.clone(),
        sampler_config,
        n_ctx: context_length,
        stop_tokens: stop_words_vec,
        use_embeddings: false,
    };

    // Add debug print for original caller_ptr
    println!("[DEBUG] create_chat_worker - Original caller_ptr: {:?}", caller_ptr);

    let adapter = ChatAdapter {
        caller: Arc::new(Mutex::new(caller_ptr)),
        token_callback: on_token,
        response_callback: on_complete,
        error_callback: on_error,
    };

    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(4096);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .build()
        .expect("Failed to create Tokio runtime");

    runtime.spawn(async move {
        chat::simple_chat_loop(params, system_prompt, msg_rx, Box::new(adapter))
            .await
            .unwrap_or_else(|e| {
                // TODO: find a way to propegate the error to c#
                println!("[ERROR] create_embedding_worker - Error: {}", e);
                ()
            });
    });
    Box::into_raw(Box::new(ChatContext {
        msg_tx,
        runtime,
    })) as *mut c_void
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

    let msg_tx = chat_context.msg_tx.clone();
    let runtime = &chat_context.runtime;
    runtime.spawn(async move { msg_tx.send(ChatMsg::Say(prompt_str)).await });
}

#[no_mangle]
pub extern "C" fn destroy_chat_worker(context: *mut c_void) {
    unsafe {
        drop(Box::from_raw(context as *mut ChatContext));
    }
}

#[cfg(test)]
mod tests {
    const TIMEOUT: u64 = 60*5;
    macro_rules! test_model_path {
        () => {
            std::env::var("TEST_MODEL")
                .unwrap_or("model.gguf".to_string())
                .as_str()
        };
    }
    
    macro_rules! test_embeddings_model_path {
        () => {
            std::env::var("TEST_EMBEDDINGS_MODEL")
                .unwrap_or("embeddings.gguf".to_string())
                .as_str()
        };
    }

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

        let model_path = CString::new(test_model_path!()).unwrap();
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
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT);

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

        let model_path = CString::new(test_model_path!()).unwrap();
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
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT);
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

        let model_path = CString::new(test_embeddings_model_path!()).unwrap();
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

        let model_path = CString::new(test_embeddings_model_path!()).unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);

        let embedding_context = create_embedding_worker(model, error_ptr);

        let text = CString::new("Hello, world!").unwrap();
        embed_text(embedding_context, text.as_ptr(), error_ptr);

        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT);
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

        let model_path = CString::new(test_embeddings_model_path!()).unwrap();
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

            let timeout = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT);
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
