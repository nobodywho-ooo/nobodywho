//! WebAssembly binding for NobodyWho.
//!
//! Mirrors the Python binding's API for JS/TS consumers. Async-only, since a
//! browser tab has no thread to block on. See `README.md` for build instructions.
//!
//! # Status: scaffold
//!
//! `wasm-pack build --target web` will fail until two upstream changes land:
//!
//! 1. The `llama-cpp-2` fork at `marek-hradil/llama-cpp-rs` (pinned at
//!    `core/Cargo.toml:15`) gains a wasm32 build path. We'll carry our own
//!    fork as a patch carrier until it's upstreamed.
//! 2. `nobodywho/core` gates its `std::thread::spawn`, `ureq` downloads, and
//!    tokio `rt-multi-thread` usage behind `cfg(not(target_arch = "wasm32"))`,
//!    and exposes a `get_model_from_bytes` constructor (no filesystem in a
//!    browser tab).
//!
//! The shape of the binding (this file) is independent of both blockers — it
//! compiles natively as an `rlib` so the workspace stays healthy. Only the
//! wasm32 build needs the upstream work.

use std::sync::Arc;
use wasm_bindgen::prelude::*;

/// Install panic hook and tracing subscriber. Call once from JS before any
/// other API. Safe to call multiple times.
#[wasm_bindgen(js_name = init)]
pub fn init() {
    console_error_panic_hook::set_once();
    #[cfg(target_arch = "wasm32")]
    {
        let _ = tracing::subscriber::set_global_default(tracing_wasm::WASMLayer::new(
            tracing_wasm::WASMLayerConfig::default(),
        ));
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
    #[wasm_bindgen(js_name = loadBytes)]
    pub async fn load_bytes(_bytes: Vec<u8>) -> Result<Model, JsError> {
        // TODO(wasm-step-2): requires a `get_model_from_bytes` constructor in
        // `nobodywho::llm` that bypasses both the ureq download path and the
        // `LlamaModelParams` file-load assumption. Tracked in Step 2 of the
        // WASM binding plan.
        Err(JsError::new(
            "Model.loadBytes is not implemented yet — requires core wasm cfg-gating (Step 2). \
             See nobodywho/wasm/README.md.",
        ))
    }
}

// ---------- Chat ----------

/// Chat session backed by a model. Manages conversation state, sampling, and tools.
#[wasm_bindgen]
pub struct Chat {
    handle: nobodywho::chat::ChatHandleAsync,
}

/// Optional config passed to the `Chat` constructor. Pass as a plain JS object:
/// `new Chat(model, { contextSize: 4096, systemPrompt: "You are helpful." })`.
#[derive(serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ChatOptions {
    context_size: Option<u32>,
    system_prompt: Option<String>,
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

        Ok(Chat {
            handle: builder.build_async(),
        })
    }

    /// Send a prompt and receive a `TokenStream`. Tokens arrive as they're
    /// generated; await `nextToken()` in a loop, or call `completed()` to
    /// resolve to the full response.
    pub async fn ask(&self, prompt: String) -> TokenStream {
        let stream = self.handle.ask(prompt);
        TokenStream {
            inner: Arc::new(tokio::sync::Mutex::new(stream)),
        }
    }

    /// Reset the conversation. Optionally provide a new system prompt.
    /// Tools are cleared on reset.
    pub async fn reset(&self, system_prompt: Option<String>) -> Result<(), JsError> {
        self.handle
            .reset_chat(system_prompt, vec![])
            .await
            .map_err(|e| JsError::new(&e.to_string()))
    }

    /// Clear the chat history, keeping the system prompt and tools.
    #[wasm_bindgen(js_name = resetHistory)]
    pub async fn reset_history(&self) -> Result<(), JsError> {
        self.handle
            .reset_history()
            .await
            .map_err(|e| JsError::new(&e.to_string()))
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
    pub async fn next_token(&self) -> Option<String> {
        self.inner.lock().await.next_token().await
    }

    /// Drain the stream and resolve to the full generated text.
    pub async fn completed(&self) -> Result<String, JsError> {
        self.inner
            .lock()
            .await
            .completed()
            .await
            .map_err(|e| JsError::new(&e.to_string()))
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
    pub async fn encode(&self, text: String) -> Result<Vec<f32>, JsError> {
        self.inner
            .encode(text)
            .await
            .map_err(|e| JsError::new(&e.to_string()))
    }
}

// ---------- Out of scope for v1 ----------
//
// The following are intentionally not yet wrapped. Each requires either a core
// API change or a wasm-specific design decision:
//
// - `CrossEncoder` / reranking — straightforward, follow the Encoder pattern.
// - `Constraint` / structured output — depends on `core/src/sampler_config.rs`
//   `GrammarConstraint`; pass-through via serde-wasm-bindgen, but llguidance
//   needs to compile to wasm32 first (Step 1).
// - Tool calling — same dependency on llguidance.
// - Multimodal (image / audio assets) — `mtmd` doesn't build for wasm32 today.
// - Progress callbacks during model load — moot since we load from `Uint8Array`.
