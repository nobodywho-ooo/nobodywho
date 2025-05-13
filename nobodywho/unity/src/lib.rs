use memory_stats::memory_stats;
use nobodywho::chat;
use nobodywho::chat::ChatMsg;
use nobodywho::llm;
use nobodywho::sampler_config::SamplerConfig;
use std::ffi::CString;
use std::ffi::{c_char, c_void, CStr};
use tokio::sync::oneshot;
use tracing::debug;
//////////////// DEBUGGING  ///////////////////

#[no_mangle]
pub extern "C" fn get_memory_stats(out_stats: *mut u64) {
    if out_stats.is_null() {
        return;
    }
    let stats = memory_stats().unwrap();
    unsafe {
        *out_stats.add(0) = stats.physical_mem as u64;
        *out_stats.add(1) = stats.virtual_mem as u64;
    }
}

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

static INIT: std::sync::Once = std::sync::Once::new();

/// Initialize tracing for tests
#[no_mangle]
pub extern "C" fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .with_timer(tracing_subscriber::fmt::time::uptime())
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
            .try_init()
            .ok();
    });
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
        Ok(Self { model })
    }
}

#[no_mangle]
pub extern "C" fn get_model(
    ptr: *mut c_void,
    path: *const c_char,
    use_gpu: bool,
    error_buf: *mut c_char,
) -> *mut c_void {
    match unsafe { (ptr as *mut ModelObject).as_ref() } {
        Some(_) => return ptr as *mut c_void,
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
            return Box::into_raw(Box::new(model_object)) as *mut c_void;
        }
    };
}

#[no_mangle]
pub extern "C" fn destroy_model(model: *mut c_void) {
    unsafe {
        drop(Box::from_raw(model as *mut ModelObject));
    }
}

/////////////////////  EMBEDDING  /////////////////////

struct EmbeddingContext {
    embed_text_tx: tokio::sync::mpsc::Sender<String>,
    embedding_result_rx: std::sync::mpsc::Receiver<Vec<f32>>,
    runtime: tokio::runtime::Runtime, // Keep this to avoid dropping the worker thread.
}

struct EmbeddingAdapter {
    embedding_result_tx: std::sync::mpsc::Sender<Vec<f32>>, // TODO: we should implenment an error callback
}

unsafe impl Send for EmbeddingAdapter {}

impl chat::EmbeddingOutput for EmbeddingAdapter {
    fn emit_embedding(&mut self, embd: Vec<f32>) {
        match self.embedding_result_tx.send(embd) {
            Ok(_) => (),
            Err(e) => {
                println!("[ERROR] EmbeddingAdapter::emit_embedding - Error: {}", e);
            }
        }
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
    error_buf: *mut c_char,
) -> *mut c_void {
    if model_ptr.is_null() {
        copy_to_error_buf(error_buf, "Model pointer is null");
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

    let (embedding_result_tx, embedding_result_rx) = std::sync::mpsc::channel();
    let adapter = EmbeddingAdapter {
        embedding_result_tx,
    };

    let (embed_text_tx, embed_text_rx) = tokio::sync::mpsc::channel(4096);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .build()
        .expect("Failed to create Tokio runtime");

    runtime.spawn(async move {
        chat::simple_embedding_loop(params, embed_text_rx, Box::new(adapter))
            .await
            .unwrap_or_else(|e| {
                // TODO: find a way to propegate the error to c#
                println!("[ERROR] create_embedding_worker - Error: {}", e);
                ()
            });
    });
    Box::into_raw(Box::new(EmbeddingContext {
        embed_text_tx,
        embedding_result_rx,
        runtime,
    })) as *mut c_void
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

    // TODO: propegate an error message
    match embedding_context.embed_text_tx.blocking_send(text_str) {
        Ok(_) => (),
        Err(e) => {
            copy_to_error_buf(
                error_buf,
                &format!("Failed to send text to embedding worker: {}", e),
            );
        }
    }
}

#[repr(C)]
pub struct FloatArray {
    data: *mut f32,
    length: usize,
}

impl FloatArray {
    // Helper to create a "None" representation
    fn none() -> Self {
        FloatArray {
            data: std::ptr::null_mut(),
            length: 0,
        }
    }

    // Helper to create a "Some" representation
    fn some(values: &[f32]) -> Self {
        let mut vec = values.to_vec();
        let result = FloatArray {
            data: vec.as_mut_ptr(),
            length: vec.len(),
        };
        std::mem::forget(vec);
        result
    }
}

#[no_mangle]
pub extern "C" fn destroy_float_array(array: FloatArray) {
    if !array.data.is_null() && array.length > 0 {
        unsafe {
            let _ = Vec::from_raw_parts(array.data, array.length, array.length);
        }
    }
}

#[no_mangle]
pub extern "C" fn poll_embed_result(context: *mut c_void) -> FloatArray {
    let embedding_context = unsafe { &mut *(context as *mut EmbeddingContext) };
    match embedding_context.embedding_result_rx.try_recv() {
        Ok(embd) => FloatArray::some(&embd),
        // TODO: handle actual errors versus empty channel errors
        Err(_) => FloatArray::none(),
    }
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
    token_rx: std::sync::mpsc::Receiver<String>,
    response_rx: std::sync::mpsc::Receiver<String>,
    error_rx: std::sync::mpsc::Receiver<String>,
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
    token_tx: std::sync::mpsc::Sender<String>,
    response_tx: std::sync::mpsc::Sender<String>,
    error_tx: std::sync::mpsc::Sender<String>,
}

unsafe impl Send for ChatAdapter {}

impl chat::ChatOutput for ChatAdapter {
    // Blockingly waits for the caller
    fn emit_token(&mut self, token: String) {
        let _ = match self.token_tx.send(token) {
            Ok(_) => (),
            Err(e) => {
                println!("[ERROR] ChatAdapter::emit_token - Error: {}", e);
            }
        };
    }

    fn emit_response(&mut self, resp: String) {
        let _ = match self.response_tx.send(resp) {
            Ok(_) => (),
            Err(e) => {
                println!("[ERROR] ChatAdapter::emit_response - Error: {}", e);
            }
        };
    }

    fn emit_error(&mut self, err: String) {
        let _ = match self.error_tx.send(err) {
            Ok(_) => (),
            Err(e) => {
                println!("[ERROR] ChatAdapter::emit_error - Error: {}", e);
            }
        };
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
    // println!("[DEBUG] create_chat_worker - Original caller_id: {:?}", caller_id); // Removed old debug print

    let (token_tx, token_rx) = std::sync::mpsc::channel();
    let (response_tx, response_rx) = std::sync::mpsc::channel();
    let (error_tx, error_rx) = std::sync::mpsc::channel();
    let adapter = ChatAdapter {
        token_tx,
        response_tx,
        error_tx,
    };

    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(4096);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .build()
        .expect("Failed to create Tokio runtime");

    let (tx, rx) = oneshot::channel::<()>();
    runtime.spawn(async move {
        chat::simple_chat_loop(params, system_prompt, msg_rx, Box::new(adapter), tx)
            .await
            .unwrap_or_else(|_e| {
                // TODO: find a way to propegate the error to c#
                ()
            });
    });
    // wait for the worker to be initialized
    let _ = rx.blocking_recv();

    Box::into_raw(Box::new(ChatContext {
        msg_tx,
        token_rx,
        response_rx,
        error_rx,
        runtime,
    })) as *mut c_void
}

fn poll_str_channel(chan: &std::sync::mpsc::Receiver<String>) -> *mut c_char {
    match chan.try_recv() {
        Ok(token) => CString::new(token)
            // TODO: handle panic
            .expect("found null bytes in token")
            .into_raw(),
        // TODO: handle other error vs empty channel error
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn poll_token(context: *mut c_void) -> *mut c_char {
    let chat_context = unsafe { &mut *(context as *mut ChatContext) };
    poll_str_channel(&chat_context.token_rx)
}

#[no_mangle]
pub extern "C" fn poll_response(context: *mut c_void) -> *mut c_char {
    let chat_context = unsafe { &mut *(context as *mut ChatContext) };
    poll_str_channel(&chat_context.response_rx)
}

#[no_mangle]
pub extern "C" fn poll_error(context: *mut c_void) -> *mut c_char {
    let chat_context = unsafe { &mut *(context as *mut ChatContext) };
    poll_str_channel(&chat_context.error_rx)
}

#[no_mangle]
pub extern "C" fn destroy_string(s: *mut c_char) {
    if s.is_null() {
        return;
    }
    unsafe {
        // Retake ownership of the CString pointer to allow Rust to deallocate it
        // when `_cstring` goes out of scope at the end of this block.
        let _cstring = CString::from_raw(s);
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

    let msg_tx = chat_context.msg_tx.clone();
    let runtime = &chat_context.runtime;
    runtime.spawn(async move {
        msg_tx
            .send(ChatMsg::Say(prompt_str))
            .await
            .unwrap_or_else(|e| {
                // we cant use the error buff here due to await
                panic!("Failed to send prompt: {}", e);
            });
    });
}

#[no_mangle]
pub extern "C" fn destroy_chat_worker(context: *mut c_void) {
    unsafe {
        drop(Box::from_raw(context as *mut ChatContext));
    }
}

/// These tests aim to cover a lot of the interface exposed with some notable omissions:
/// - We cannot test across the ffi barrier, so native c# code and callbacks will have to be emulated
/// - Life times act different based on wheter we are lopping some thing over a language barrier
///   or invoking all the methods from Rust.
#[cfg(test)]
mod tests {
    const TIMEOUT: u64 = 60 * 5;
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
    use std::thread;
    use std::time::Duration;

    static mut EMBEDDING: Option<Vec<f32>> = None;
    static mut LAST_ERROR: Option<String> = None;
    static mut DUMMY_CALLER_DATA: u8 = 0;

    extern "C" fn _embed_on_embedding(caller: *const c_void, data: *const f32, length: i32) {
        println!(
            "[TEST_DEBUG] _embed_on_embedding - Received embedding for caller {:?}, length: {}",
            caller, length
        );
        unsafe {
            if data.is_null() || length <= 0 {
                println!(
                    "[TEST_ERROR] _embed_on_embedding - Received null or empty embedding data"
                );
                LAST_ERROR = Some("Received null or empty embedding".to_string());
                return;
            }
            // Create Vec from raw parts
            let embedding_slice = std::slice::from_raw_parts(data, length as usize);
            EMBEDDING = Some(embedding_slice.to_vec());
        }
    }

    #[test]
    fn test_create_chat_worker_with_stop_tokens() {
        init_tracing();
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

        let mut accumulated_response = String::new();
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT);
        let mut final_response_received = false;

        while std::time::Instant::now() < timeout {
            let error_c_str = poll_error(chat_context);
            if !error_c_str.is_null() {
                let error_str = unsafe {
                    CStr::from_ptr(error_c_str)
                        .to_str()
                        .unwrap_or_default()
                        .to_owned()
                };
                destroy_string(error_c_str);
                panic!("Chat worker errored: {}", error_str);
            }

            let token_c_str = poll_token(chat_context);
            if !token_c_str.is_null() {
                let token_str = unsafe { CStr::from_ptr(token_c_str).to_str().unwrap_or_default() };
                accumulated_response.push_str(token_str);
                destroy_string(token_c_str);
            }

            let response_c_str = poll_response(chat_context);
            if !response_c_str.is_null() {
                // The final response might be the full string or just a confirmation.
                // We're primarily interested that it signals completion.
                // If needed, you could compare/append this to accumulated_response.
                let final_str = unsafe {
                    CStr::from_ptr(response_c_str)
                        .to_str()
                        .unwrap_or_default()
                        .to_owned()
                };
                if accumulated_response.is_empty() && !final_str.is_empty() {
                    accumulated_response = final_str;
                }
                destroy_string(response_c_str);
                final_response_received = true;
                break;
            }
            thread::sleep(Duration::from_millis(1)); // Polling interval
        }

        if !final_response_received {
            panic!("Timed out waiting for response");
        }

        debug!(
            "Full response for stop token test: {}",
            accumulated_response
        );
        assert!(
            accumulated_response.to_lowercase().contains("cat"),
            "Response should contain cat. Got: {}",
            accumulated_response
        );
        assert!(
            accumulated_response.to_lowercase().contains("dog"),
            "Response should contain dog. Got: {}",
            accumulated_response
        );
        // Depending on exact model behavior with stop tokens, "fly" might or might not be included.
        // If "fly" is a hard stop, it shouldn't be in the output. If it's processed then stopped, it might be.
        // For this test, let's assume it might appear before stopping.
        assert!(
            accumulated_response.to_lowercase().contains("fly"),
            "Response should contain fly. Got: {}",
            accumulated_response
        );
        assert!(
            !accumulated_response.to_lowercase().contains("lion"),
            "Response should stop before lion. Got: {}",
            accumulated_response
        );

        destroy_chat_worker(chat_context);
        destroy_model(model);
    }

    #[test]
    fn test_create_chat_worker() {
        init_tracing();
        let error_buf = [0u8; 2048];
        let error_ptr = error_buf.as_ptr() as *mut c_char;

        let model_path = CString::new(test_model_path!()).unwrap();
        let model: *mut c_void =
            get_model(std::ptr::null_mut(), model_path.as_ptr(), true, error_ptr);

        let system_prompt = CString::new("You are a test assistant").unwrap();
        let context_length: u32 = 4096;

        let chat_context: *mut c_void = create_chat_worker(
            model,
            system_prompt.as_ptr(),
            std::ptr::null(), // No stop words
            context_length,
            false,            // No grammar
            std::ptr::null(), // No grammar
            error_ptr,
        );

        let prompt = CString::new("Hello, how are you?").unwrap();
        send_prompt(chat_context, prompt.as_ptr(), error_ptr);
        assert_eq!(
            unsafe { CStr::from_ptr(error_ptr).to_bytes() },
            &[0u8; 0],
            "Send prompt should succeed. Error: {}",
            unsafe {
                CStr::from_ptr(error_ptr)
                    .to_str()
                    .unwrap_or("Invalid error string")
            }
        );

        let mut accumulated_response = String::new();
        let timeout = std::time::Instant::now() + std::time::Duration::from_secs(TIMEOUT);
        let mut final_response_received = false;

        while std::time::Instant::now() < timeout {
            let error_c_str = poll_error(chat_context);
            if !error_c_str.is_null() {
                let error_str = unsafe {
                    CStr::from_ptr(error_c_str)
                        .to_str()
                        .unwrap_or_default()
                        .to_owned()
                };
                destroy_string(error_c_str);
                panic!("Chat worker errored: {}", error_str);
            }

            let token_c_str = poll_token(chat_context);
            if !token_c_str.is_null() {
                let token_str = unsafe { CStr::from_ptr(token_c_str).to_str().unwrap_or_default() };
                accumulated_response.push_str(token_str);
                destroy_string(token_c_str);
            }

            let response_c_str = poll_response(chat_context);
            if !response_c_str.is_null() {
                let final_str = unsafe {
                    CStr::from_ptr(response_c_str)
                        .to_str()
                        .unwrap_or_default()
                        .to_owned()
                };
                if accumulated_response.is_empty() && !final_str.is_empty() {
                    accumulated_response = final_str;
                }
                destroy_string(response_c_str);
                final_response_received = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        assert!(
            final_response_received,
            "Should have received a final response (completion signal)"
        );
        assert!(
            !accumulated_response.is_empty(),
            "Accumulated response should not be empty"
        );
        debug!(
            "Full response for basic chat test: {}",
            accumulated_response
        );
        destroy_chat_worker(chat_context);
        destroy_model(model);
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

        let mut embd;
        loop {
            embd = poll_embed_result(embedding_context);
            if embd.length > 0 {
                break;
            }
        }
        destroy_embedding_worker(embedding_context);
        destroy_model(model as *mut c_void);
    }
}
