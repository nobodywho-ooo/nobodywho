//! WebAssembly binding for NobodyWho.
//!
//! Mirrors the Python binding's API for JS/TS consumers. Async-only, since a
//! browser tab has no thread to block on. See `README.md` for build instructions.
//!
//! # Why everything returns `js_sys::Promise` instead of being `pub async fn`
//!
//! `#[wasm_bindgen] pub async fn` desugars through
//! `wasm_bindgen_futures::future_to_promise`, which requires the future to be
//! `UnwindSafe`. Several of our types (`tokio::sync::Mutex`,
//! `tokio::sync::mpsc::Receiver`, etc.) aren't, so we'd hit
//! E0277 on every async method.
//!
//! Instead, each exported method is a plain `pub fn` returning
//! `js_sys::Promise`, with the body run through the [`promisify`] helper which
//! wraps the body in [`std::panic::AssertUnwindSafe`] + `catch_unwind`. Since
//! wasm is single-threaded and there's no real concurrent access to these
//! types, the assertion is sound — any panic is caught and surfaced to JS as
//! a rejected promise.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures::FutureExt;
use wasm_bindgen::prelude::*;


// Export `_initialize` so a WASI host can run static ctors via
// wasi.initialize(). Body is empty — wasi-libc/libc++ ctors are emitted
// into `__wasm_call_ctors`, which wasm-bindgen / node:wasi handle for us.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn _initialize() {}

/// Override wasi-libc's `__cxa_atexit` to a no-op.
///
/// The default rust-lld 22.1 wasm driver doesn't understand
/// `--mexec-model=reactor`, so it leaves the cdylib in the "command" exec
/// model: every wasm-bindgen export gets wrapped with `__wasm_call_ctors` +
/// `__wasm_call_dtors`. The dtor walk runs `__funcs_on_exit`, which iterates
/// `__cxa_atexit`-registered handlers and calls each. At least one of those
/// is registered with a wasm signature that doesn't match how
/// `__funcs_on_exit` invokes it, producing
///
/// ```text
///   RuntimeError: function signature mismatch
/// ```
///
/// on the FIRST export call after instantiation, before any of our code
/// runs. The handlers are global-destructor callbacks libc++ registers
/// during static init.
///
/// Workaround: define `__cxa_atexit` ourselves and have it ignore the
/// registration. Global destructors won't run at module shutdown (which
/// is fine — the wasm instance lives for the lifetime of the JS process
/// anyway, and the OS reclaims the heap), but the dtor walk becomes a
/// no-op and the signature-mismatch goes away.
///
/// `#[no_mangle]` puts the symbol at file scope; in the wasm link, ours
/// wins over wasi-libc's definition because rustc-emitted symbols are
/// resolved before sysroot archives.
///
/// # Safety
///
/// Declared `unsafe` because the C ABI passes a function pointer and a raw
/// `*mut c_void` argument we can neither validate nor dereference. We do
/// neither — we ignore all three arguments and return success. That makes
/// this implementation trivially safe to call from any caller (no UB
/// regardless of what handlers libc++ tries to register), at the cost of
/// silently dropping every registration. See the "Workaround:" paragraph
/// above for why dropping them is acceptable on this target.
#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub unsafe extern "C" fn __cxa_atexit(
    _func: Option<unsafe extern "C" fn(*mut std::ffi::c_void)>,
    _arg: *mut std::ffi::c_void,
    _dso_handle: *mut std::ffi::c_void,
) -> i32 {
    0 // pretend the registration succeeded; never run anything at exit.
}

// ---------- Install panic hook & tracing ----------

/// Install panic hook and tracing subscriber. Call once from JS before any
/// other API. Safe to call multiple times.
#[wasm_bindgen(js_name = init)]
pub fn init() {
    console_error_panic_hook::set_once();
    #[cfg(target_arch = "wasm32")]
    {
        // `set_as_global_default` panics if called twice; the `try_*` variant
        // returns Err which we discard, making this idempotent from JS.
        tracing_wasm::try_set_as_global_default().ok();
    }
}

// ---------- Promise helper ----------

/// Wrap a `Future<Output = Result<T, JsError>>` into a `js_sys::Promise`,
/// asserting it's unwind-safe and catching panics so they reject the promise
/// rather than tearing down the whole wasm instance.
fn promisify<F, T>(fut: F) -> js_sys::Promise
where
    F: Future<Output = Result<T, JsError>> + 'static,
    T: Into<JsValue>,
{
    let safe = AssertUnwindSafe(async move {
        match fut.await {
            Ok(v) => Ok(v.into()),
            Err(e) => Err(JsValue::from(e)),
        }
    })
    .catch_unwind()
    .map(|r| r.unwrap_or_else(|_| Err(JsValue::from_str("rust panic crossed wasm boundary"))));
    wasm_bindgen_futures::future_to_promise(safe)
}

// ---------- Model ----------

/// A loaded GGUF model. Share between `Chat` and `Encoder` instances; the
/// underlying model data is reference-counted.
#[wasm_bindgen]
pub struct Model {
    inner: Arc<nobodywho::llm::Model>,
}

#[wasm_bindgen]
impl Model {
    /// Load a model from raw GGUF bytes.
    ///
    /// Browser usage: `fetch('model.gguf').then(r => r.arrayBuffer()).then(buf => Model.loadBytes(new Uint8Array(buf)))`.
    ///
    /// There's no path-based loader in the wasm binding — the browser sandbox
    /// has no filesystem. For HuggingFace / URL fetching, do it on the JS side
    /// before calling this.
    ///
    /// CPU-only; the wasm32 target has no GPU concept. `gpu_layers` is fixed
    /// at 0 internally.
    #[wasm_bindgen(js_name = loadBytes)]
    pub fn load_bytes(bytes: Vec<u8>) -> js_sys::Promise {
        promisify(async move {
            let model = nobodywho::llm::get_model_from_bytes(&bytes, 0)
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(Model {
                inner: Arc::new(model),
            })
        })
    }
}

// ---------- Chat ----------
//
// Direct wasm-bindgen wrapper over `ChatHandleAsync`. On
// wasm32-unknown-emscripten the underlying worker is a real pthread
// (Emscripten implements pthreads via Web Workers + SharedArrayBuffer),
// so `.ask()` runs the inference off the main JS thread without any
// extra wasm-side Web-Worker plumbing — just like the Python binding's
// `Chat.ask` runs off the GIL via a thread.

/// Chat session backed by a model. Manages conversation state, sampling, and tools.
#[wasm_bindgen]
pub struct Chat {
    handle: nobodywho::chat::ChatHandleAsync,
}

/// Optional config passed to the `Chat` constructor. Pass as a plain JS object:
///
/// ```js
/// new Chat(model, {
///   contextSize: 4096,
///   systemPrompt: "You are helpful.",
///   constraint: { jsonSchema: '{"type":"object","properties":{...}}' },
/// });
/// ```
// `deny_unknown_fields` matches ConstraintSpec below and surfaces JS-side
// typos / unsupported options (e.g. `tools: [...]`) as a JsError at
// construction time, rather than serde silently dropping the field.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChatOptions {
    context_size: Option<u32>,
    system_prompt: Option<String>,
    constraint: Option<ConstraintSpec>,
    /// Variables passed to the chat template, e.g. `{ enable_thinking: false }`
    /// for Qwen-Thinking-style models that emit `<think>…</think>` blocks you
    /// don't want in the response. Mirrors Python's `template_variables`. Only
    /// boolean values are accepted (matches Python and the core template
    /// layer); a non-bool value rejects at construction time.
    template_variables: Option<std::collections::HashMap<String, bool>>,
}

/// Grammar constraint for structured-output generation, via llguidance.
///
/// Exactly one of the fields should be set. JS-side examples:
///
/// ```js
/// // JSON Schema:
/// { jsonSchema: '{"type":"object","properties":{"answer":{"type":"string"}}}' }
///
/// // Regex:
/// { regex: "[A-Z][a-z]+" }
///
/// // Lark CFG:
/// { lark: 'start: "yes" | "no"' }
/// ```
///
/// All three are documented in core's `GrammarConstraint`; this is just the
/// JS-facing wire format.
///
/// **Runtime caveat on wasm32-unknown-unknown:** llguidance currently panics
/// on `std::time::Instant::now()` (no monotonic clock), so any constraint
/// that reaches the grammar sampler will trip an upstream bug at generation
/// time. Tracked upstream; the wire format here is stable.
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConstraintSpec {
    json_schema: Option<String>,
    regex: Option<String>,
    lark: Option<String>,
}

impl ConstraintSpec {
    fn into_sampler(self) -> Result<nobodywho::sampler_config::SamplerConfig, JsError> {
        use nobodywho::sampler_config::SamplerPresets;
        let n_set = self.json_schema.is_some() as u8
            + self.regex.is_some() as u8
            + self.lark.is_some() as u8;
        if n_set != 1 {
            return Err(JsError::new(
                "constraint must set exactly one of jsonSchema / regex / lark",
            ));
        }
        Ok(if let Some(s) = self.json_schema {
            SamplerPresets::constrain_with_json_schema(s)
        } else if let Some(p) = self.regex {
            SamplerPresets::constrain_with_regex(p)
        } else {
            SamplerPresets::constrain_with_grammar(self.lark.unwrap())
        })
    }
}

#[wasm_bindgen]
impl Chat {
    /// Create a new chat session bound to a loaded model.
    #[wasm_bindgen(constructor)]
    pub fn new(model: &Model, options: JsValue) -> Result<Chat, JsError> {
        let opts: ChatOptions = if options.is_undefined() || options.is_null() {
            ChatOptions::default()
        } else {
            serde_wasm_bindgen::from_value(options).map_err(|e| JsError::new(&e.to_string()))?
        };

        let mut builder = nobodywho::chat::ChatBuilder::new(Arc::clone(&model.inner));
        if let Some(ctx) = opts.context_size {
            builder = builder.with_context_size(ctx);
        }
        if let Some(sys) = opts.system_prompt {
            builder = builder.with_system_prompt(Some(sys));
        }
        if let Some(constraint) = opts.constraint {
            builder = builder.with_sampler(constraint.into_sampler()?);
        }
        if let Some(vars) = opts.template_variables {
            builder = builder.with_template_variables(vars);
        }

        Ok(Chat {
            handle: builder.build_async(),
        })
    }

    /// Send a prompt and receive a `TokenStream`. Await `nextToken()` in a
    /// loop, or call `completed()` to resolve to the full response.
    ///
    /// **Wasm: blocks the calling thread until generation completes.** The
    /// inference loop is synchronous Rust that doesn't yield back to the JS
    /// event loop between tokens, so the `nextToken()` Promises only resolve
    /// AFTER the full response is generated — the channel buffer drains in
    /// one go. If you call `ask` from the main JS thread the page freezes
    /// for the duration. For non-blocking inference, spawn a Web Worker on
    /// the JS side that imports the Emscripten loader and call `ask` from
    /// inside the Worker — the Worker's event loop blocks during inference,
    /// not the main thread.
    pub fn ask(&self, prompt: String) -> js_sys::Promise {
        let handle = self.handle.clone();
        promisify(async move {
            let stream = handle.ask(prompt);
            Ok(TokenStream {
                inner: Arc::new(tokio::sync::Mutex::new(stream)),
            })
        })
    }

    /// Send a prompt and stream tokens via a JS callback called per token,
    /// rather than via the channel-based `TokenStream`. Returns a Promise that
    /// resolves to the full response when generation completes.
    ///
    /// Under Emscripten with pthreads, the inference loop runs on a real
    /// pthread, so `ask()` already streams tokens as they're produced (the
    /// pthread isn't the main JS thread). This method is sugar for
    /// "do the next_token loop in Rust so the JS side only sees one callback
    /// per token plus a final Promise resolution."
    ///
    /// JS usage:
    /// ```js
    /// const full = await chat.askStreaming(prompt, (tok) => write(tok));
    /// ```
    #[wasm_bindgen(js_name = askStreaming)]
    pub fn ask_streaming(&self, prompt: String, on_token: js_sys::Function) -> js_sys::Promise {
        let handle = self.handle.clone();
        promisify(async move {
            let mut stream = handle.ask(prompt);
            while let Some(token) = stream.next_token().await {
                // Errors from the JS callback are swallowed (matches the old
                // synchronous-hook behaviour). If a user wants to know about
                // them, they can wrap the callback in a try/catch.
                let _ = on_token.call1(&JsValue::null(), &JsValue::from_str(&token));
            }
            let full = stream
                .completed()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::from_str(&full))
        })
    }

    /// Reset the conversation. Optionally provide a new system prompt.
    /// (Tools aren't yet exposed in the wasm binding.)
    pub fn reset(&self, system_prompt: Option<String>) -> js_sys::Promise {
        let handle = self.handle.clone();
        promisify(async move {
            handle
                .reset_chat(system_prompt, vec![])
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Clear the chat history, keeping the system prompt and tools.
    #[wasm_bindgen(js_name = resetHistory)]
    pub fn reset_history(&self) -> js_sys::Promise {
        let handle = self.handle.clone();
        promisify(async move {
            handle
                .reset_history()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }
}

// ---------- TokenStream ----------

/// An in-progress text completion. JS usage:
///
/// ```js
/// const stream = await chat.ask("Hello");
/// let tok;
/// while ((tok = await stream.nextToken()) !== undefined) {
///   process.stdout.write(tok);
/// }
/// ```
///
/// `Symbol.asyncIterator` is intentionally not exposed yet — the `nextToken()`
/// loop is portable across all JS runtimes including older Node versions.
/// Adding it later is non-breaking.
#[wasm_bindgen]
pub struct TokenStream {
    inner: Arc<tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>>,
}

#[wasm_bindgen]
impl TokenStream {
    /// Resolves to the next token, or `undefined` when generation ends.
    #[wasm_bindgen(js_name = nextToken)]
    pub fn next_token(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let token = inner.lock().await.next_token().await;
            // Map `Option<String>` to JsValue: Some -> string, None -> undefined.
            // This is how JS callers detect end-of-stream.
            Ok(match token {
                Some(s) => JsValue::from_str(&s),
                None => JsValue::UNDEFINED,
            })
        })
    }

    /// Drain the stream and resolve to the full generated text.
    pub fn completed(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            inner
                .lock()
                .await
                .completed()
                .await
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }
}

// ---------- Encoder ----------

/// Generate embedding vectors. Requires an embedding model (not a chat model).
#[wasm_bindgen]
pub struct Encoder {
    inner: nobodywho::encoder::EncoderAsync,
}

#[wasm_bindgen]
impl Encoder {
    /// Create a new encoder. `n_ctx` defaults to 4096 if omitted.
    #[wasm_bindgen(constructor)]
    pub fn new(model: &Model, n_ctx: Option<u32>) -> Encoder {
        Encoder {
            inner: nobodywho::encoder::EncoderAsync::new(
                Arc::clone(&model.inner),
                n_ctx.unwrap_or(4096),
            ),
        }
    }

    /// Generate an embedding vector for the given text. Resolves to a
    /// `Float32Array` on the JS side.
    pub fn encode(&self, text: String) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let embedding = inner
                .encode(text)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            // Convert Vec<f32> to Float32Array. The `js_sys::Float32Array::from`
            // copies into a fresh wasm-allocated typed array.
            Ok(JsValue::from(js_sys::Float32Array::from(
                embedding.as_slice(),
            )))
        })
    }
}

// ---------- CrossEncoder ----------

/// Cross-encoder for relevance-ranking documents against a query. Requires a
/// cross-encoder model (e.g. a BGE reranker GGUF), not a chat or embedding
/// model.
#[wasm_bindgen]
pub struct CrossEncoder {
    inner: nobodywho::crossencoder::CrossEncoderAsync,
}

#[wasm_bindgen]
impl CrossEncoder {
    /// Create a new cross-encoder. `n_ctx` defaults to 4096 if omitted.
    #[wasm_bindgen(constructor)]
    pub fn new(model: &Model, n_ctx: Option<u32>) -> CrossEncoder {
        CrossEncoder {
            inner: nobodywho::crossencoder::CrossEncoderAsync::new(
                Arc::clone(&model.inner),
                n_ctx.unwrap_or(4096),
            ),
        }
    }

    /// Score each document against the query. Resolves to a `Float32Array`
    /// in the same order as the input documents.
    pub fn rank(&self, query: String, documents: Vec<String>) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let scores = inner
                .rank(query, documents)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::from(js_sys::Float32Array::from(scores.as_slice())))
        })
    }

    /// Score each document and resolve to an array of `[document, score]`
    /// pairs sorted by descending score. Mirrors Python's
    /// `CrossEncoder.rank_and_sort(...)` -> `list[tuple[str, float]]`.
    #[wasm_bindgen(js_name = rankAndSort)]
    pub fn rank_and_sort(&self, query: String, documents: Vec<String>) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let ranked = inner
                .rank_and_sort(query, documents)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            // Build `[[doc, score], ...]` as nested `js_sys::Array`. Returning a
            // Vec<(String, f32)> directly would need serde_wasm_bindgen and the
            // JS side would see plain Objects; nested Arrays keep the wire
            // format obvious (`for (const [doc, score] of result)`).
            let outer = js_sys::Array::new_with_length(ranked.len() as u32);
            for (i, (doc, score)) in ranked.into_iter().enumerate() {
                let pair = js_sys::Array::new_with_length(2);
                pair.set(0, JsValue::from_str(&doc));
                pair.set(1, JsValue::from_f64(score as f64));
                outer.set(i as u32, pair.into());
            }
            Ok(JsValue::from(outer))
        })
    }
}

// ---------- Cache API helpers ----------
//
// Browser-side model caching via the Cache API store named 'nobodywho-models-v1'.
// Used to live in examples/setup-browser.mjs (~80 lines of JS); now lives here
// via web-sys so the JS-side bootstrap stays a thin shim. Same store name as
// before, so existing cached models survive the JS→Rust migration.

#[cfg(target_arch = "wasm32")]
const MODEL_CACHE_NAME: &str = "nobodywho-models-v1";

/// Try to open the model cache. Returns None if the Cache API isn't usable
/// in the current context (insecure http, file://, sandboxed iframe) — the
/// caller falls through to a plain fetch in that case.
///
/// `caches` is available on both `Window` (main thread) and
/// `WorkerGlobalScope` (web worker), with different web-sys types.
#[cfg(target_arch = "wasm32")]
async fn open_model_cache() -> Option<web_sys::Cache> {
    let caches = caches_from_global()?;
    let opened = wasm_bindgen_futures::JsFuture::from(caches.open(MODEL_CACHE_NAME))
        .await
        .ok()?;
    opened.dyn_into::<web_sys::Cache>().ok()
}

#[cfg(target_arch = "wasm32")]
fn caches_from_global() -> Option<web_sys::CacheStorage> {
    if let Ok(window) = js_sys::global().dyn_into::<web_sys::Window>() {
        return window.caches().ok();
    }
    if let Ok(scope) = js_sys::global().dyn_into::<web_sys::WorkerGlobalScope>() {
        return scope.caches().ok();
    }
    None
}

#[cfg(target_arch = "wasm32")]
fn fetch_from_global(url: &str) -> js_sys::Promise {
    if let Ok(window) = js_sys::global().dyn_into::<web_sys::Window>() {
        return window.fetch_with_str(url);
    }
    if let Ok(scope) = js_sys::global().dyn_into::<web_sys::WorkerGlobalScope>() {
        return scope.fetch_with_str(url);
    }
    js_sys::Promise::reject(&JsValue::from_str(
        "fetch() not available in this global context",
    ))
}

/// Fetch a GGUF from a URL and resolve to its bytes, with optional progress
/// reporting and Cache API caching. JS-exposed as `fetchModelBytes`.
///
/// Mirrors the deleted JS implementation. Cache hit returns immediately
/// (firing `onProgress(size, size)` once for UIs that only update on
/// progress events). Cache miss streams the body so progress is meaningful;
/// on completion, populates the cache (swallows put failures).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = fetchModelBytes)]
pub fn fetch_model_bytes(
    url: String,
    on_progress: Option<js_sys::Function>,
) -> js_sys::Promise {
    promisify(async move {
        // --- Cache lookup ---
        if let Some(cache) = open_model_cache().await {
            let matched = wasm_bindgen_futures::JsFuture::from(cache.match_with_str(&url))
                .await
                .ok();
            if let Some(matched_val) = matched {
                if !matched_val.is_undefined() {
                    let response: web_sys::Response = matched_val
                        .dyn_into()
                        .map_err(|_| JsError::new("cache hit returned non-Response"))?;
                    let array_buffer_promise = response
                        .array_buffer()
                        .map_err(|e| JsError::new(&format!("array_buffer(): {e:?}")))?;
                    let array_buffer = wasm_bindgen_futures::JsFuture::from(array_buffer_promise)
                        .await
                        .map_err(|e| JsError::new(&format!("array_buffer await: {e:?}")))?;
                    let bytes = js_sys::Uint8Array::new(&array_buffer);
                    if let Some(cb) = on_progress.as_ref() {
                        let len = JsValue::from_f64(bytes.byte_length() as f64);
                        let _ = cb.call2(&JsValue::null(), &len, &len);
                    }
                    return Ok(bytes);
                }
            }
        }

        // --- Cache miss: fetch from network ---
        let response_jsval = wasm_bindgen_futures::JsFuture::from(fetch_from_global(&url))
            .await
            .map_err(|e| JsError::new(&format!("fetch failed: {e:?}")))?;
        let response: web_sys::Response = response_jsval
            .dyn_into()
            .map_err(|_| JsError::new("fetch did not return a Response"))?;
        if !response.ok() {
            return Err(JsError::new(&format!(
                "fetch {url}: HTTP {} {}",
                response.status(),
                response.status_text()
            )));
        }
        let total: u64 = response
            .headers()
            .get("content-length")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        let body = response
            .body()
            .ok_or_else(|| JsError::new("response.body is null"))?;
        let reader: web_sys::ReadableStreamDefaultReader = body
            .get_reader()
            .dyn_into()
            .map_err(|_| JsError::new("expected ReadableStreamDefaultReader"))?;

        let mut chunks: Vec<u8> = Vec::with_capacity(total.max(1) as usize);
        let mut downloaded: u64 = 0;
        loop {
            let read_result =
                wasm_bindgen_futures::JsFuture::from(reader.read())
                    .await
                    .map_err(|e| JsError::new(&format!("reader.read(): {e:?}")))?;
            let done = js_sys::Reflect::get(&read_result, &"done".into())
                .map_err(|_| JsError::new("read result missing 'done'"))?
                .as_bool()
                .unwrap_or(false);
            if done {
                break;
            }
            let value = js_sys::Reflect::get(&read_result, &"value".into())
                .map_err(|_| JsError::new("read result missing 'value'"))?;
            let chunk: js_sys::Uint8Array = value
                .dyn_into()
                .map_err(|_| JsError::new("read chunk is not a Uint8Array"))?;
            let chunk_len = chunk.byte_length() as usize;
            let start = chunks.len();
            chunks.resize(start + chunk_len, 0);
            chunk.copy_to(&mut chunks[start..]);
            downloaded += chunk_len as u64;
            if let Some(cb) = on_progress.as_ref() {
                let _ = cb.call2(
                    &JsValue::null(),
                    &JsValue::from_f64(downloaded as f64),
                    &JsValue::from_f64(total as f64),
                );
            }
        }

        let bytes = js_sys::Uint8Array::from(&chunks[..]);

        // --- Populate cache (best-effort) ---
        if let Some(cache) = open_model_cache().await {
            if let Ok(resp) = web_sys::Response::new_with_opt_buffer_source(Some(&bytes)) {
                let _ = wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&url, &resp))
                    .await;
            }
        }

        Ok(bytes)
    })
}

// Static methods on the existing Model class. wasm-bindgen lets you add to
// the same JS class from multiple `impl` blocks.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Model {
    /// Pre-populate the Cache API model store without loading the model into
    /// wasm. Useful during a splash screen so the user isn't waiting on a
    /// 400 MB download when they click "chat".
    #[wasm_bindgen(static_method_of = Model, js_name = preload)]
    pub fn preload(url: String, on_progress: Option<js_sys::Function>) -> js_sys::Promise {
        promisify(async move {
            let _ = wasm_bindgen_futures::JsFuture::from(fetch_model_bytes(url, on_progress))
                .await
                .map_err(|e| JsError::new(&format!("preload: {e:?}")))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Delete the model cache store. Resolves to `true` if a store existed
    /// and was removed, `false` otherwise (no Cache API in this context, or
    /// the store didn't exist).
    #[wasm_bindgen(static_method_of = Model, js_name = clearCache)]
    pub fn clear_cache() -> js_sys::Promise {
        promisify(async move {
            let Some(caches) = caches_from_global() else {
                return Ok(JsValue::from_bool(false));
            };
            let result = wasm_bindgen_futures::JsFuture::from(caches.delete(MODEL_CACHE_NAME))
                .await
                .map_err(|e| JsError::new(&format!("caches.delete: {e:?}")))?;
            Ok(JsValue::from_bool(result.as_bool().unwrap_or(false)))
        })
    }
}

// ---------- Out of scope for v1 ----------
//
// The following are intentionally not yet wrapped. Each requires either a core
// API change or a wasm-specific design decision:
//
// - Tool calling — depends on llguidance behavior on wasm.
// - Multimodal (image / audio assets) — `mtmd` is not currently enabled on wasm.
// - Progress callbacks during model load — moot since we load from `Uint8Array`
//   (the browser-side `fetchModelBytes` helper in `examples/setup-browser.mjs`
//   reports its own download progress via JS).
//
// Grammar-constrained generation IS wired through `Chat::new`'s options —
// see `ConstraintSpec` above for the wire format and the runtime caveat.
// `CrossEncoder` IS wired — see the section above.
