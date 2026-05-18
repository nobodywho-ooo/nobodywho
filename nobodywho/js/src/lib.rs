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

// Per-worker state for `runInWorker` — only used on wasm32 targets.
#[cfg(target_arch = "wasm32")]
use std::cell::RefCell;

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

// ---------- Streaming hook RAII (wasm32 only) ----------
//
// RAII guard for the per-thread streaming hook in `core::llm`. `install`
// refuses to overwrite an existing hook — overlapping `askStreaming` calls
// can't share the slot because the chat worker processes asks FIFO but
// installs are LIFO, so two concurrent installs would misroute tokens. See
// the doc on `core::llm::set_streaming_hook` for the underlying reason.
// Drop unconditionally clears, which is safe because we only construct when
// the slot was empty before our write.
#[cfg(target_arch = "wasm32")]
struct HookRestore;
#[cfg(target_arch = "wasm32")]
impl HookRestore {
    fn install(hook: Box<dyn Fn(&str)>) -> Result<Self, JsError> {
        let displaced = nobodywho::llm::set_streaming_hook(Some(hook));
        if displaced.is_some() {
            // Put it back. set_streaming_hook returns Some(our_hook) here,
            // which we drop on the floor — fine, because we're erroring out
            // and the on_token capture inside it is no longer needed.
            let _ = nobodywho::llm::set_streaming_hook(displaced);
            return Err(JsError::new(
                "askStreaming: another streaming call is already in progress; \
                 await the previous askStreaming promise before starting a new one",
            ));
        }
        Ok(Self)
    }
}
#[cfg(target_arch = "wasm32")]
impl Drop for HookRestore {
    fn drop(&mut self) {
        // `install` only constructs Self when the slot was empty before our
        // write, so the slot stays "ours alone" for our lifetime (single-
        // threaded wasm guarantees no other code can observe or mutate it
        // while this future is suspended on an await). Clearing to None on
        // drop is therefore equivalent to "restore previous."
        nobodywho::llm::set_streaming_hook(None);
    }
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

// ---------- ChatRaw (raw blocking class — see Chat below for the user-facing one) ----------
//
// Exposed in JS as `ChatRaw`. The user-facing worker-backed wrapper is the
// `Chat` struct further down; `ChatRaw` is the direct wasm-bindgen wrapper
// over `ChatHandleAsync` that the worker dispatcher uses internally and that
// advanced callers can opt into if they want blocking inference on the main
// thread.

/// Chat session backed by a model. Manages conversation state, sampling, and tools.
#[wasm_bindgen]
pub struct ChatRaw {
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
impl ChatRaw {
    /// Create a new chat session bound to a loaded model.
    #[wasm_bindgen(constructor)]
    pub fn new(model: &Model, options: JsValue) -> Result<ChatRaw, JsError> {
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

        Ok(ChatRaw {
            handle: builder.build_async(),
        })
    }

    /// Send a prompt and receive a `TokenStream`. Await `nextToken()` in a
    /// loop, or call `completed()` to resolve to the full response.
    ///
    /// **Wasm note: this does NOT stream in real time.** The Rust inference
    /// loop holds the single JS thread until generation completes, so the
    /// `nextToken()` loop only drains tokens AFTER the response is fully
    /// generated. To see tokens arrive as they're produced, run the wasm in
    /// a Web Worker and use [`Chat::ask_streaming`] (`askStreaming` in JS),
    /// which calls a JS callback synchronously from inside the inference loop
    /// — the callback can then `self.postMessage(token)` to the main thread.
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
    /// On wasm32, sync inference inside the wasm holds the worker thread
    /// until completion, so a `chat.ask(...).then((stream) => stream.nextToken())`
    /// loop only sees tokens AFTER the whole generation finishes. This method
    /// instead installs a synchronous streaming hook that's called from inside
    /// the inference loop — the JS callback runs there and can
    /// `self.postMessage(token)` from a Web Worker, which is non-blocking, so
    /// the main thread sees tokens as they're produced.
    ///
    /// **Concurrency: one streaming call per thread at a time.** If a previous
    /// `askStreaming` is still in flight when this is called, the returned
    /// Promise rejects with an "already in progress" error — await the
    /// previous one first. The constraint is per-thread, so it also applies
    /// across `Chat` instances that share a thread (e.g. two `Chat`s in the
    /// same Web Worker).
    ///
    /// **Don't mix with `ask` mid-flight.** The worker fires the streaming
    /// hook for every generated token regardless of which API initiated the
    /// Ask, so an in-flight (non-streaming) `ask`'s tokens would be misrouted
    /// through a later `askStreaming` callback. Serialize: drain the
    /// `TokenStream` (or await its `completed()`) before starting an
    /// `askStreaming` on the same `Chat`.
    ///
    /// JS usage from inside a Worker:
    /// ```js
    /// chat.askStreaming(prompt, (tok) => self.postMessage({type: 'token', token: tok}))
    ///   .then((full) => self.postMessage({type: 'done', full}));
    /// ```
    #[wasm_bindgen(js_name = askStreaming)]
    pub fn ask_streaming(&self, prompt: String, on_token: js_sys::Function) -> js_sys::Promise {
        let handle = self.handle.clone();
        promisify(async move {
            // Install the per-thread streaming hook for the duration of this
            // call. `install` fails if another streaming call is already in
            // flight on this thread — propagate that as a rejected Promise
            // rather than silently misrouting tokens.
            #[cfg(target_arch = "wasm32")]
            let _restore = HookRestore::install(Box::new(move |tok| {
                let _ = on_token.call1(&JsValue::null(), &JsValue::from_str(tok));
            }))?;
            #[cfg(not(target_arch = "wasm32"))]
            let _ = &on_token;

            let mut stream = handle.ask(prompt);
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

// ---------- Worker dispatcher ----------
//
// The browser-side `Chat` wrapper in `examples/setup-browser.mjs` spawns a
// Web Worker and talks to it over a small message protocol. The dispatcher
// for that protocol used to live in `examples/worker.js` (~50 lines of JS).
// Now it lives here — `runInWorker()` sets up `self.onmessage` and reacts
// to `load-model` / `create-chat` / `ask` messages, posting `model-loaded`
// / `chat-ready` / `token` / `ask-done` / `error` back. The worker file
// itself is just `import './setup-browser.mjs'` — setup-browser.mjs calls
// `runInWorker` when it detects DedicatedWorkerGlobalScope.
//
// Per-instance state lives in `thread_local!` because wasm32 is
// single-threaded (one wasm instance per worker = one cell).

#[cfg(target_arch = "wasm32")]
thread_local! {
    static WORKER_MODEL: RefCell<Option<Arc<nobodywho::llm::Model>>> = RefCell::new(None);
    static WORKER_CHAT: RefCell<Option<nobodywho::chat::ChatHandleAsync>> = RefCell::new(None);
}

/// Take over `self.onmessage` for the Web Worker that hosts this wasm
/// instance. Idempotent only in the sense that JS-side guards in
/// `setup-browser.mjs` won't call it twice; calling it twice from Rust would
/// install two closures and the second would overwrite the first's
/// `set_onmessage` (which is also fine — the first closure leaks, the
/// second is now the handler).
///
/// Errors if invoked outside a Web Worker (e.g. on the main thread).
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = runInWorker)]
pub fn run_in_worker() -> Result<(), JsError> {
    use wasm_bindgen::closure::Closure;
    use web_sys::{DedicatedWorkerGlobalScope, MessageEvent};

    let scope: DedicatedWorkerGlobalScope = js_sys::global().dyn_into().map_err(|_| {
        JsError::new("runInWorker must be called inside a DedicatedWorkerGlobalScope")
    })?;

    let scope_for_handler = scope.clone();
    let on_message = Closure::wrap(Box::new(move |evt: MessageEvent| {
        // Read `evt.data()` synchronously here — Firefox throws
        // NS_ERROR_NOT_AVAILABLE if you touch MessageEvent properties from an
        // async continuation that runs after the synchronous handler returns.
        // The cloned JsValue we move into spawn_local is just a regular JS
        // value and safe to read whenever.
        let data = evt.data();
        let scope = scope_for_handler.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(err) = handle_worker_message(data, &scope).await {
                let _ = scope.post_message(&worker_reply_error(&err));
            }
        });
    }) as Box<dyn FnMut(MessageEvent)>);

    scope.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
    // Leak: the closure outlives this function and runs for the worker's
    // lifetime. The worker is terminated by main-thread `worker.terminate()`
    // or page navigation, both of which tear down the wasm instance anyway.
    on_message.forget();

    let _ = scope.post_message(&worker_reply("ready"));
    Ok(())
}

/// One per message-type. Returning `Err` is what produces the `error` reply
/// — the caller wraps it via `worker_reply_error` and posts that. Takes the
/// already-extracted `data` JsValue (not the raw `MessageEvent`) because
/// Firefox revokes access to event properties once the synchronous handler
/// returns — see the comment on the `set_onmessage` call site.
#[cfg(target_arch = "wasm32")]
async fn handle_worker_message(
    data: JsValue,
    scope: &web_sys::DedicatedWorkerGlobalScope,
) -> Result<(), String> {
    let msg_type = js_sys::Reflect::get(&data, &"type".into())
        .map_err(|_| "missing 'type' field".to_string())?
        .as_string()
        .ok_or_else(|| "'type' must be a string".to_string())?;

    match msg_type.as_str() {
        // Back-compat: callers that post `init` right after `new Worker(...)`
        // expecting a `ready` ack. The bootstrap already posted `ready` once;
        // we re-ack here so those callers don't hang.
        "init" => {
            let _ = scope.post_message(&worker_reply("ready"));
        }
        "load-model" => {
            let bytes_val = js_sys::Reflect::get(&data, &"bytes".into())
                .map_err(|_| "missing 'bytes' field".to_string())?;
            let u8_array: js_sys::Uint8Array = bytes_val
                .dyn_into()
                .map_err(|_| "'bytes' must be a Uint8Array".to_string())?;
            let bytes = u8_array.to_vec();
            let model = nobodywho::llm::get_model_from_bytes(&bytes, 0)
                .map_err(|e| e.to_string())?;
            WORKER_MODEL.with(|m| *m.borrow_mut() = Some(Arc::new(model)));
            let _ = scope.post_message(&worker_reply("model-loaded"));
        }
        "create-chat" => {
            let options = js_sys::Reflect::get(&data, &"options".into())
                .unwrap_or(JsValue::UNDEFINED);
            let opts: ChatOptions = if options.is_undefined() || options.is_null() {
                ChatOptions::default()
            } else {
                serde_wasm_bindgen::from_value(options).map_err(|e| e.to_string())?
            };
            let model = WORKER_MODEL
                .with(|m| m.borrow().clone())
                .ok_or_else(|| "model not loaded; send 'load-model' first".to_string())?;
            let handle = chat_handle_from_options(model, opts)?;
            WORKER_CHAT.with(|c| *c.borrow_mut() = Some(handle));
            let _ = scope.post_message(&worker_reply("chat-ready"));
        }
        "ask" => {
            let prompt = js_sys::Reflect::get(&data, &"prompt".into())
                .map_err(|_| "missing 'prompt' field".to_string())?
                .as_string()
                .ok_or_else(|| "'prompt' must be a string".to_string())?;
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created; send 'create-chat' first".to_string())?;

            // Install a streaming hook that turns each generated token into a
            // postMessage. Held for the duration of this ask via `_restore`;
            // dropped on exit so a subsequent ask can install its own hook.
            // `HookRestore::install` rejects if another ask is already running
            // on this thread — the main thread shouldn't allow this, but if it
            // somehow does the error surfaces clearly here.
            let scope_for_hook = scope.clone();
            let _restore = HookRestore::install(Box::new(move |token: &str| {
                let payload = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&payload, &"type".into(), &"token".into());
                let _ = js_sys::Reflect::set(&payload, &"token".into(), &token.into());
                let _ = scope_for_hook.post_message(&payload);
            }))
            .map_err(|_| "another ask is already in progress on this worker".to_string())?;

            let mut stream = handle.ask(prompt);
            let _full = stream.completed().await.map_err(|e| e.to_string())?;
            let _ = scope.post_message(&worker_reply("ask-done"));
        }
        other => return Err(format!("unknown msg type: {other}")),
    }

    Ok(())
}

/// Build a `ChatHandleAsync` from a parsed `ChatOptions`. Same option-mapping
/// logic as `Chat::new`'s constructor — factored out so the worker dispatcher
/// doesn't duplicate it. Errors as `String` because the worker dispatcher
/// turns them into `{ type: "error", message }` post-messages; `JsError`
/// (used by the wasm-bindgen-exposed constructor) doesn't impl `Display`.
#[cfg(target_arch = "wasm32")]
fn chat_handle_from_options(
    model: Arc<nobodywho::llm::Model>,
    opts: ChatOptions,
) -> Result<nobodywho::chat::ChatHandleAsync, String> {
    let mut builder = nobodywho::chat::ChatBuilder::new(model);
    if let Some(ctx) = opts.context_size {
        builder = builder.with_context_size(ctx);
    }
    if let Some(sys) = opts.system_prompt {
        builder = builder.with_system_prompt(Some(sys));
    }
    if let Some(constraint) = opts.constraint {
        // ConstraintSpec::into_sampler returns Err(JsError) only when the
        // exclusive-one-of check fails; reach into the JsError's underlying
        // Error.message via Reflect.
        let sampler = constraint.into_sampler().map_err(|e| {
            let val: JsValue = e.into();
            js_sys::Reflect::get(&val, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
                .unwrap_or_else(|| "invalid constraint".to_string())
        })?;
        builder = builder.with_sampler(sampler);
    }
    if let Some(vars) = opts.template_variables {
        builder = builder.with_template_variables(vars);
    }
    Ok(builder.build_async())
}

#[cfg(target_arch = "wasm32")]
fn worker_reply(type_name: &str) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"type".into(), &type_name.into());
    obj.into()
}

#[cfg(target_arch = "wasm32")]
fn worker_reply_error(message: &str) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"type".into(), &"error".into());
    let _ = js_sys::Reflect::set(&obj, &"message".into(), &message.into());
    obj.into()
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

// ---------- WorkerTokenStream ----------
//
// User-facing token stream returned from `Chat::ask`. Implements the JS
// async-iterator protocol (`next()` returning `Promise<{value, done}>`) so
// callers can `for await (const tok of chat.ask(...))`, and exposes
// `completed()` returning a `Promise<string>` that resolves to the full
// concatenation.
//
// `[Symbol.asyncIterator]() { return this; }` can't be emitted by
// wasm-bindgen 0.2.121 cleanly; setup-browser.mjs adds it at the prototype
// level after import (~1 line of JS).
//
// State shared with `Chat` via `Rc<RefCell<WorkerStreamState>>`: Chat pushes
// tokens/done/error into the state from inside its `onmessage` closure; the
// stream's `next()`/`completed()` Promises resolve out of that state.

#[cfg(target_arch = "wasm32")]
struct WorkerStreamState {
    /// Tokens that have arrived but haven't been pulled by `next()`.
    buffer: std::collections::VecDeque<String>,
    /// Accumulated text — `completed()` resolves to this.
    full_text: String,
    /// Set when the worker posts `ask-done`.
    done: bool,
    /// Set when the worker posts `error` or the worker errors.
    error: Option<String>,
    /// If `next()` is pending (no buffered token and not done/error), this
    /// is the sender to fulfill when the next token / done / error arrives.
    pending_next: Option<tokio::sync::oneshot::Sender<NextOutcome>>,
    /// `completed()` may be called multiple times; queue all waiters.
    pending_completed: Vec<tokio::sync::oneshot::Sender<Result<String, String>>>,
}

#[cfg(target_arch = "wasm32")]
enum NextOutcome {
    Token(String),
    Done,
    Err(String),
}

#[cfg(target_arch = "wasm32")]
impl WorkerStreamState {
    fn new() -> Self {
        Self {
            buffer: std::collections::VecDeque::new(),
            full_text: String::new(),
            done: false,
            error: None,
            pending_next: None,
            pending_completed: Vec::new(),
        }
    }

    fn push_token(state: &std::rc::Rc<RefCell<Self>>, token: String) {
        let mut st = state.borrow_mut();
        st.full_text.push_str(&token);
        if let Some(tx) = st.pending_next.take() {
            let _ = tx.send(NextOutcome::Token(token));
        } else {
            st.buffer.push_back(token);
        }
    }

    fn complete(state: &std::rc::Rc<RefCell<Self>>) {
        let mut st = state.borrow_mut();
        st.done = true;
        if let Some(tx) = st.pending_next.take() {
            let _ = tx.send(NextOutcome::Done);
        }
        let full = st.full_text.clone();
        for tx in st.pending_completed.drain(..) {
            let _ = tx.send(Ok(full.clone()));
        }
    }

    fn fail(state: &std::rc::Rc<RefCell<Self>>, err: String) {
        let mut st = state.borrow_mut();
        st.error = Some(err.clone());
        if let Some(tx) = st.pending_next.take() {
            let _ = tx.send(NextOutcome::Err(err.clone()));
        }
        for tx in st.pending_completed.drain(..) {
            let _ = tx.send(Err(err.clone()));
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct WorkerTokenStream {
    state: std::rc::Rc<RefCell<WorkerStreamState>>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl WorkerTokenStream {
    /// Returns `Promise<{ value: string, done: false }>` for each token,
    /// or `Promise<{ value: undefined, done: true }>` when the stream ends.
    /// Rejects with the worker's error if the inference failed.
    pub fn next(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            // Synchronous fast-path: check buffer / done / error.
            let pending_rx = {
                let mut st = state.borrow_mut();
                if let Some(err) = st.error.clone() {
                    return Err(JsError::new(&err));
                }
                if let Some(tok) = st.buffer.pop_front() {
                    return Ok(iter_result(Some(&tok)));
                }
                if st.done {
                    return Ok(iter_result(None));
                }
                let (tx, rx) = tokio::sync::oneshot::channel();
                st.pending_next = Some(tx);
                rx
            };
            // Async: park until Chat's onmessage routes a token / done / error.
            match pending_rx.await {
                Ok(NextOutcome::Token(tok)) => Ok(iter_result(Some(&tok))),
                Ok(NextOutcome::Done) => Ok(iter_result(None)),
                Ok(NextOutcome::Err(e)) => Err(JsError::new(&e)),
                Err(_) => Err(JsError::new("stream sender dropped before token")),
            }
        })
    }

    /// Resolves to the full accumulated text once the stream completes.
    /// Calling `completed()` multiple times is fine — each call queues an
    /// independent waiter resolved with the same final text.
    pub fn completed(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            let pending_rx = {
                let mut st = state.borrow_mut();
                if let Some(err) = st.error.clone() {
                    return Err(JsError::new(&err));
                }
                if st.done {
                    return Ok(JsValue::from_str(&st.full_text));
                }
                let (tx, rx) = tokio::sync::oneshot::channel();
                st.pending_completed.push(tx);
                rx
            };
            match pending_rx.await {
                Ok(Ok(text)) => Ok(JsValue::from_str(&text)),
                Ok(Err(e)) => Err(JsError::new(&e)),
                Err(_) => Err(JsError::new("stream sender dropped before completion")),
            }
        })
    }
}

/// Build a `{ value, done }` JS object matching the async-iterator protocol.
#[cfg(target_arch = "wasm32")]
fn iter_result(value: Option<&str>) -> JsValue {
    let obj = js_sys::Object::new();
    match value {
        Some(v) => {
            let _ = js_sys::Reflect::set(&obj, &"value".into(), &JsValue::from_str(v));
            let _ = js_sys::Reflect::set(&obj, &"done".into(), &JsValue::from_bool(false));
        }
        None => {
            let _ = js_sys::Reflect::set(&obj, &"value".into(), &JsValue::UNDEFINED);
            let _ = js_sys::Reflect::set(&obj, &"done".into(), &JsValue::from_bool(true));
        }
    }
    obj.into()
}

// ---------- Chat (worker-backed, user-facing) ----------
//
// User-facing chat class. Internally spawns a Web Worker from an inline Blob
// URL, posts the load-model / create-chat / ask protocol, routes replies via
// a Closure-wrapped onmessage. The worker side of the protocol is handled by
// `runInWorker()` further up.
//
// The JS-side Chat class that used to live in examples/setup-browser.mjs is
// now this Rust struct. App code is unchanged — same factory shape:
//
//     const chat = await Chat.create({ modelUrl, systemPrompt, ... });
//     for await (const tok of chat.ask(prompt)) { ... }
//     const full = await chat.ask(prompt).completed();

// JS sets this at module load (`bg.setBootstrapUrl(import.meta.url)`).
// `Chat::create` reads it to build the inline Blob worker bootstrap
// (`import('<setup-browser.mjs URL>')`).
#[cfg(target_arch = "wasm32")]
thread_local! {
    static BOOTSTRAP_URL: RefCell<Option<String>> = RefCell::new(None);
}

/// Called from setup-browser.mjs at module load to register the absolute URL
/// of setup-browser.mjs itself. Required before the first `Chat.create()`.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen(js_name = setBootstrapUrl)]
pub fn set_bootstrap_url(url: String) {
    BOOTSTRAP_URL.with(|u| *u.borrow_mut() = Some(url));
}

#[cfg(target_arch = "wasm32")]
fn get_bootstrap_url() -> Result<String, JsError> {
    BOOTSTRAP_URL
        .with(|u| u.borrow().clone())
        .ok_or_else(|| {
            JsError::new(
                "Chat.create: setBootstrapUrl was not called. \
                 setup-browser.mjs must call bg.setBootstrapUrl(import.meta.url) \
                 at module load before Chat.create() is invoked.",
            )
        })
}

#[cfg(target_arch = "wasm32")]
struct ChatState {
    worker: web_sys::Worker,
    current_stream: Option<std::rc::Rc<RefCell<WorkerStreamState>>>,
    /// While `Chat::create` is running through its load-model / create-chat
    /// handshake, this holds `(expected_reply_type, sender)`. The onmessage
    /// closure resolves the sender when a message of that type arrives.
    pending_handshake:
        Option<(String, tokio::sync::oneshot::Sender<Result<(), String>>)>,
    terminated: bool,
    _on_message: Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>>,
    _on_error: Option<wasm_bindgen::closure::Closure<dyn FnMut(web_sys::ErrorEvent)>>,
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub struct Chat {
    state: std::rc::Rc<RefCell<ChatState>>,
}

#[cfg(target_arch = "wasm32")]
impl Drop for Chat {
    fn drop(&mut self) {
        // Best-effort cleanup: terminate the worker so it doesn't hang around
        // after the wasm-side Chat is released. The closures hold `Weak`
        // refs to ChatState (no cycle), so dropping state here is safe.
        if let Ok(st) = self.state.try_borrow() {
            let _ = st.worker.terminate();
        }
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
impl Chat {
    /// Create a worker-backed chat. Spawns a Web Worker that loads the wasm,
    /// fetches the model (Cache API hit if previously downloaded), and
    /// initializes a `ChatHandleAsync`. Returns a Promise that resolves to
    /// the `Chat` instance once all three handshake steps complete.
    #[wasm_bindgen(js_name = create)]
    pub fn create(opts: JsValue) -> js_sys::Promise {
        promisify(async move {
            let parsed = parse_chat_create_opts(&opts)?;
            let bootstrap = get_bootstrap_url()?;

            // Build the inline-Blob worker entrypoint.
            // JSON-encode the URL so it's safely interpolated as a string literal.
            let url_lit = serde_json::to_string(&bootstrap)
                .map_err(|e| JsError::new(&format!("json url: {e}")))?;
            let bootstrap_code = format!("import({url_lit});");

            let blob_bag = web_sys::BlobPropertyBag::new();
            blob_bag.set_type("text/javascript");
            let parts = js_sys::Array::new();
            parts.push(&JsValue::from_str(&bootstrap_code));
            let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &blob_bag)
                .map_err(|e| JsError::new(&format!("new Blob: {e:?}")))?;
            let url = web_sys::Url::create_object_url_with_blob(&blob)
                .map_err(|e| JsError::new(&format!("createObjectURL: {e:?}")))?;

            let worker_opts = web_sys::WorkerOptions::new();
            worker_opts.set_type(web_sys::WorkerType::Module);
            let worker = web_sys::Worker::new_with_options(&url, &worker_opts)
                .map_err(|e| JsError::new(&format!("new Worker: {e:?}")))?;

            // Construct the state. Closures install themselves into state.
            let state = std::rc::Rc::new(RefCell::new(ChatState {
                worker: worker.clone(),
                current_stream: None,
                pending_handshake: None,
                terminated: false,
                _on_message: None,
                _on_error: None,
            }));

            let state_weak = std::rc::Rc::downgrade(&state);
            let on_message = wasm_bindgen::closure::Closure::wrap(Box::new(
                move |evt: web_sys::MessageEvent| {
                    if let Some(state) = state_weak.upgrade() {
                        handle_chat_message(&state, evt.data());
                    }
                },
            )
                as Box<dyn FnMut(web_sys::MessageEvent)>);

            let state_weak2 = std::rc::Rc::downgrade(&state);
            let on_error = wasm_bindgen::closure::Closure::wrap(Box::new(
                move |evt: web_sys::ErrorEvent| {
                    if let Some(state) = state_weak2.upgrade() {
                        let msg = format!("worker crashed: {}", evt.message());
                        handle_chat_error(&state, msg);
                    }
                },
            )
                as Box<dyn FnMut(web_sys::ErrorEvent)>);

            worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
            worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));

            {
                let mut st = state.borrow_mut();
                st._on_message = Some(on_message);
                st._on_error = Some(on_error);
            }

            // Handshake step 1: wait for 'ready' from the worker.
            wait_for_handshake(&state, "ready").await?;

            // Handshake step 2: get the model bytes, post 'load-model'.
            let model_bytes_value: JsValue = if let Some(bytes) = parsed.model_bytes {
                bytes.into()
            } else if let Some(url) = parsed.model_url {
                let bytes_promise = fetch_model_bytes(url, parsed.on_progress);
                wasm_bindgen_futures::JsFuture::from(bytes_promise)
                    .await
                    .map_err(|e| {
                        let msg = js_sys::Reflect::get(&e, &"message".into())
                            .ok()
                            .and_then(|m| m.as_string())
                            .unwrap_or_else(|| format!("{e:?}"));
                        JsError::new(&format!("fetchModelBytes: {msg}"))
                    })?
            } else {
                return Err(JsError::new(
                    "Chat.create: pass either modelUrl or modelBytes",
                ));
            };

            let load_msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&load_msg, &"type".into(), &"load-model".into());
            let _ = js_sys::Reflect::set(&load_msg, &"bytes".into(), &model_bytes_value);
            state
                .borrow()
                .worker
                .post_message(&load_msg)
                .map_err(|e| JsError::new(&format!("post load-model: {e:?}")))?;
            wait_for_handshake(&state, "model-loaded").await?;

            // Handshake step 3: post 'create-chat' with the chat options
            // (the original JS object minus the modelUrl/modelBytes/
            // onDownloadProgress keys; see parse_chat_create_opts).
            let create_msg = js_sys::Object::new();
            let _ =
                js_sys::Reflect::set(&create_msg, &"type".into(), &"create-chat".into());
            let _ =
                js_sys::Reflect::set(&create_msg, &"options".into(), &parsed.chat_opts_jsval);
            state
                .borrow()
                .worker
                .post_message(&create_msg)
                .map_err(|e| JsError::new(&format!("post create-chat: {e:?}")))?;
            wait_for_handshake(&state, "chat-ready").await?;

            Ok(Chat { state })
        })
    }

    /// Send a prompt; returns a synchronously-constructed `WorkerTokenStream`
    /// that resolves token-by-token (or all-at-once via `.completed()`).
    /// Only one ask can be in flight at a time per Chat.
    pub fn ask(&self, prompt: String) -> Result<WorkerTokenStream, JsError> {
        let mut st = self.state.borrow_mut();
        if st.terminated {
            return Err(JsError::new("Chat: already terminated"));
        }
        if st.current_stream.is_some() {
            return Err(JsError::new(
                "Chat.ask: another ask is in progress; await its .completed() or finish iterating first",
            ));
        }

        let stream_state = std::rc::Rc::new(RefCell::new(WorkerStreamState::new()));
        st.current_stream = Some(stream_state.clone());

        let ask_msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&ask_msg, &"type".into(), &"ask".into());
        let _ = js_sys::Reflect::set(&ask_msg, &"prompt".into(), &JsValue::from_str(&prompt));
        st.worker
            .post_message(&ask_msg)
            .map_err(|e| JsError::new(&format!("post ask: {e:?}")))?;
        drop(st);

        Ok(WorkerTokenStream {
            state: stream_state,
        })
    }

    /// Shut down the worker. Any in-flight stream is failed with "terminated";
    /// subsequent calls to `ask` reject.
    pub fn terminate(&self) {
        let mut st = self.state.borrow_mut();
        if st.terminated {
            return;
        }
        st.terminated = true;
        let stream = st.current_stream.take();
        let _ = st.worker.terminate();
        drop(st);
        if let Some(s) = stream {
            WorkerStreamState::fail(&s, "Chat terminated".to_string());
        }
    }
}

/// Synchronous router for messages from the chat worker. Runs from inside
/// the onmessage Closure. Borrow rules: take what you need, then drop the
/// borrow before invoking `WorkerStreamState::*` helpers (which take their
/// own borrow on the stream's inner state).
#[cfg(target_arch = "wasm32")]
fn handle_chat_message(state: &std::rc::Rc<RefCell<ChatState>>, data: JsValue) {
    let msg_type = js_sys::Reflect::get(&data, &"type".into())
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();

    match msg_type.as_str() {
        "token" => {
            let token = js_sys::Reflect::get(&data, &"token".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            let stream = state.borrow().current_stream.clone();
            if let Some(s) = stream {
                WorkerStreamState::push_token(&s, token);
            }
        }
        "ask-done" => {
            let stream = state.borrow_mut().current_stream.take();
            if let Some(s) = stream {
                WorkerStreamState::complete(&s);
            }
        }
        "error" => {
            let err_msg = js_sys::Reflect::get(&data, &"message".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_else(|| "unknown worker error".to_string());
            handle_chat_error(state, err_msg);
        }
        // Handshake reply: resolve a pending oneshot if its expected type matches.
        other => {
            let mut st = state.borrow_mut();
            let take_it = matches!(&st.pending_handshake, Some((t, _)) if t == other);
            if take_it {
                if let Some((_, tx)) = st.pending_handshake.take() {
                    let _ = tx.send(Ok(()));
                }
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn handle_chat_error(state: &std::rc::Rc<RefCell<ChatState>>, err: String) {
    // Fail current handshake or current stream — whichever is active.
    let (handshake, stream) = {
        let mut st = state.borrow_mut();
        (st.pending_handshake.take(), st.current_stream.take())
    };
    if let Some((_, tx)) = handshake {
        let _ = tx.send(Err(err.clone()));
    }
    if let Some(s) = stream {
        WorkerStreamState::fail(&s, err);
    }
}

/// Park until the worker posts a message of the given type (or errors out).
#[cfg(target_arch = "wasm32")]
async fn wait_for_handshake(
    state: &std::rc::Rc<RefCell<ChatState>>,
    expected_type: &str,
) -> Result<(), JsError> {
    let rx = {
        let mut st = state.borrow_mut();
        // Sanity: the previous handshake should be settled.
        if st.pending_handshake.is_some() {
            return Err(JsError::new(
                "wait_for_handshake called while another handshake is pending (internal bug)",
            ));
        }
        let (tx, rx) = tokio::sync::oneshot::channel();
        st.pending_handshake = Some((expected_type.to_string(), tx));
        rx
    };
    match rx.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(JsError::new(&e)),
        Err(_) => Err(JsError::new(&format!(
            "handshake sender dropped before {expected_type}"
        ))),
    }
}

/// Parsed Chat.create options. `chat_opts_jsval` is the original JS object
/// minus the modelUrl / modelBytes / onDownloadProgress keys — passed
/// through to the worker as-is via postMessage. We do NOT re-serialize via
/// `serde_wasm_bindgen::to_value(&ChatOptions)` because that converts nested
/// maps (e.g. `templateVariables: { enable_thinking: false }`) into JS Maps,
/// and the worker's `serde_wasm_bindgen::from_value` round-trip doesn't
/// always preserve the original Object-vs-Map shape — small differences
/// caused `templateVariables` to silently come through empty.
#[cfg(target_arch = "wasm32")]
struct ChatCreateParsed {
    model_url: Option<String>,
    model_bytes: Option<js_sys::Uint8Array>,
    on_progress: Option<js_sys::Function>,
    chat_opts_jsval: JsValue,
}

#[cfg(target_arch = "wasm32")]
fn parse_chat_create_opts(opts: &JsValue) -> Result<ChatCreateParsed, JsError> {
    if opts.is_undefined() || opts.is_null() {
        return Err(JsError::new("Chat.create requires an options object"));
    }
    let obj = opts
        .dyn_ref::<js_sys::Object>()
        .ok_or_else(|| JsError::new("Chat.create options must be an object"))?;

    let model_url = js_sys::Reflect::get(obj, &"modelUrl".into())
        .ok()
        .and_then(|v| v.as_string());
    let model_bytes = js_sys::Reflect::get(obj, &"modelBytes".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok());
    let on_progress = js_sys::Reflect::get(obj, &"onDownloadProgress".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());

    // Build a filtered JS object containing only the ChatOptions fields.
    let chat_opts_obj = js_sys::Object::new();
    let keys = js_sys::Object::keys(obj);
    for k in keys.iter() {
        let key_str = k.as_string().unwrap_or_default();
        if matches!(
            key_str.as_str(),
            "modelUrl" | "modelBytes" | "onDownloadProgress"
        ) {
            continue;
        }
        if let Ok(v) = js_sys::Reflect::get(obj, &k) {
            let _ = js_sys::Reflect::set(&chat_opts_obj, &k, &v);
        }
    }

    // Validate by attempting to parse to ChatOptions. We don't keep the
    // result — we pass the raw JS object to the worker — but parsing here
    // catches typos and unsupported fields (`deny_unknown_fields`) at
    // create time rather than at chat-creation time inside the worker.
    if js_sys::Object::keys(&chat_opts_obj).length() > 0 {
        let _: ChatOptions = serde_wasm_bindgen::from_value(chat_opts_obj.clone().into())
            .map_err(|e| JsError::new(&format!("Chat.create options: {e}")))?;
    }

    Ok(ChatCreateParsed {
        model_url,
        model_bytes,
        on_progress,
        chat_opts_jsval: chat_opts_obj.into(),
    })
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
