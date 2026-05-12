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

// Override wasi-libc's `__cxa_atexit` to a no-op.
//
// The default rust-lld 22.1 wasm driver doesn't understand
// `--mexec-model=reactor`, so it leaves the cdylib in the "command" exec
// model: every wasm-bindgen export gets wrapped with __wasm_call_ctors +
// __wasm_call_dtors. The dtor walk runs `__funcs_on_exit`, which iterates
// `__cxa_atexit`-registered handlers and calls each. At least one of those
// is registered with a wasm signature that doesn't match how
// __funcs_on_exit invokes it, producing
//
//   RuntimeError: function signature mismatch
//
// on the FIRST export call after instantiation, before any of our code
// runs. The handlers are global-destructor callbacks libc++ registers
// during static init.
//
// Workaround: define `__cxa_atexit` ourselves and have it ignore the
// registration. Global destructors won't run at module shutdown (which
// is fine — the wasm instance lives for the lifetime of the JS process
// anyway, and the OS reclaims the heap), but the dtor walk becomes a
// no-op and the signature-mismatch goes away.
//
// `#[no_mangle]` puts the symbol at file scope; in the wasm link, ours
// wins over wasi-libc's definition because rustc-emitted symbols are
// resolved before sysroot archives.
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
    .map(|r| {
        r.unwrap_or_else(|_| Err(JsValue::from_str("rust panic crossed wasm boundary")))
    });
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
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ChatOptions {
    context_size: Option<u32>,
    system_prompt: Option<String>,
    constraint: Option<ConstraintSpec>,
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
        let n_set =
            self.json_schema.is_some() as u8 + self.regex.is_some() as u8 + self.lark.is_some() as u8;
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

        Ok(Chat {
            handle: builder.build_async(),
        })
    }

    /// Send a prompt and receive a `TokenStream`. Tokens arrive as they're
    /// generated; await `nextToken()` in a loop, or call `completed()` to
    /// resolve to the full response.
    pub fn ask(&self, prompt: String) -> js_sys::Promise {
        let handle = self.handle.clone();
        promisify(async move {
            let stream = handle.ask(prompt);
            Ok(TokenStream {
                inner: Arc::new(tokio::sync::Mutex::new(stream)),
            })
        })
    }

    /// Reset the conversation. Optionally provide a new system prompt.
    /// Tools are cleared on reset.
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

// ---------- Out of scope for v1 ----------
//
// The following are intentionally not yet wrapped. Each requires either a core
// API change or a wasm-specific design decision:
//
// - `CrossEncoder` / reranking — straightforward, follow the Encoder pattern.
// - `Constraint` / structured output — depends on `core/src/sampler_config.rs`
//   `GrammarConstraint`; pass-through via serde-wasm-bindgen.
// - Tool calling — depends on llguidance behavior on wasm.
// - Multimodal (image / audio assets) — `mtmd` is not currently enabled on wasm.
// - Progress callbacks during model load — moot since we load from `Uint8Array`.
