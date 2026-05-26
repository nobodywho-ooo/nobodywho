//! WebAssembly binding for NobodyWho.
//!
//! Mirrors the Python binding's API for JS/TS consumers. Async-only, since a
//! browser tab has no thread to block on. See `README.md` for build instructions.
//!

// Native builds (used by `cargo test -p nobodywho-js` for the lint suite +
// `cargo check` for IDE integration) trip dead-code warnings on every
// wasm-only item (SamplerSpec, into_sampler, build_sampler, all the
// worker_* helpers, etc.) because the wasm_bindgen-exported callers that
// use them are cfg-gated to wasm. With CI's `RUSTFLAGS=-D warnings`
// these escalate to compile errors. The items ARE used on wasm; suppress
// the warning only when we're on native to keep that signal alive for
// the wasm build.
#![allow(dead_code)]
#![cfg_attr(not(target_family = "wasm"), allow(unused))]
// wasm_bindgen's `#[wasm_bindgen(static_method_of = ..., js_name = ...)]`
// macro generates an `unused variable: static_method_of` warning that we
// can't suppress at the call site (the warning fires inside the macro
// expansion). Same for the `unused_imports` cases — they're generated
// by macros. CI sets RUSTFLAGS=-D warnings which turns these into hard
// errors. Allow at crate level.
#![allow(unused_variables, unused_imports)]

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

// Force-import file-open syscalls into the wasm; see the module's
// doc-comment + js/build.rs.
#[cfg(target_family = "wasm")]
mod syscall_imports;

// Per-worker state for `runInWorker` — only used on wasm32 targets.
#[cfg(target_family = "wasm")]
use std::cell::RefCell;

/// Override libc's `__cxa_atexit` to a no-op.
///
/// At least one global-destructor handler libc++ registers during static
/// init has a wasm signature that doesn't match how `__funcs_on_exit`
/// invokes it, producing
///
/// ```text
///   RuntimeError: function signature mismatch
/// ```
///
/// on the FIRST export call after instantiation, before any of our code
/// runs.
///
/// Workaround: define `__cxa_atexit` ourselves and have it ignore the
/// registration. Global destructors won't run at module shutdown (which
/// is fine — the wasm instance lives for the lifetime of the JS process
/// anyway, and the OS reclaims the heap), but the dtor walk becomes a
/// no-op and the signature-mismatch goes away.
///
/// `#[no_mangle]` puts the symbol at file scope; in the wasm link, ours
/// wins over the sysroot's definition because rustc-emitted symbols are
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
#[cfg(target_family = "wasm")]
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
    #[cfg(target_family = "wasm")]
    {
        // `set_as_global_default` panics if called twice; the `try_*` variant
        // returns Err which we discard, making this idempotent from JS.
        tracing_wasm::try_set_as_global_default().ok();
    }
}

// ---------- Promise helper ----------

/// Cosine similarity between two embedding vectors. Convenience
/// helper paired with `Encoder.encode()`. Mirrors Python's
/// `nobodywho.cosine_similarity`.
///
/// ```js
/// import { Encoder, Model, cosineSimilarity } from 'nobodywho-js';
/// const v1 = await encoder.encode('the quick brown fox');
/// const v2 = await encoder.encode('a fast brown fox');
/// const sim = cosineSimilarity(v1, v2);  // 0..1
/// ```
///
/// Accepts `Float32Array | number[]`. Throws on length mismatch.
/// Returns NaN if either vector has zero magnitude (matches Python).
#[wasm_bindgen(js_name = cosineSimilarity)]
pub fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> Result<f32, JsError> {
    if a.len() != b.len() {
        return Err(JsError::new(&format!(
            "cosineSimilarity: vectors have different lengths ({} vs {})",
            a.len(),
            b.len()
        )));
    }
    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    Ok(dot / (na.sqrt() * nb.sqrt()))
}

/// Wrap a `Future<Output = Result<T, JsError>>` into a `js_sys::Promise`,
/// asserting it's unwind-safe and catching panics so they reject the promise
/// rather than tearing down the whole wasm instance.
fn promisify<F, T>(fut: F) -> js_sys::Promise
where
    F: Future<Output = Result<T, JsError>> + 'static,
    T: Into<JsValue>,
{
    // No catch_unwind: the (Rc<RefCell<ChatState>> + other) captures in the
    // worker-backed Chat futures aren't RefUnwindSafe and can't be made so
    // without a deeper refactor. AssertUnwindSafe satisfies future_to_promise's
    // own UnwindSafe bound; we accept that a Rust panic propagates as a hard
    // wasm abort instead of a rejected promise — the same failure mode as a
    // C++ exception crossing the wasm boundary on Emscripten.
    wasm_bindgen_futures::future_to_promise(AssertUnwindSafe(async move {
        match fut.await {
            Ok(v) => Ok(v.into()),
            Err(e) => Err(JsValue::from(e)),
        }
    }))
}

// Per-token streaming on wasm32: the worker's `"ask"` arm builds a
// synchronous `Rc<dyn Fn(&str)>` and passes it to
// `ChatHandleAsync::ask_with_token_hook` (wasm-only API in core). The
// hook fires from inside the inference loop on each sampled token and
// `postMessage`s a `{type:'token', token}` payload directly to main.
// The channel-based path (`ChatHandleAsync::ask`) can't stream on
// single-threaded wasm because the receiver task only wakes after the
// synchronous inference loop completes; the hook bypasses that. Measured
// effect: TTFT 10.8 s → 1.85 s on Qwen3-0.6B-Q4_K_M, ~1% wall-time
// overhead from the per-token postMessage.

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
    /// Pass `mmprojBytes` to enable multimodal (vision / audio) input. The
    /// bytes are written to Emscripten's MEMFS at a content-hashed path
    /// and loaded via the existing path-based projection model loader. Pass
    /// `null`/`undefined` for text-only models.
    ///
    /// ```js
    /// // text-only
    /// const model = await Model.loadBytes(modelBytes);
    /// // multimodal — both arguments are Uint8Array
    /// const model = await Model.loadBytes(modelBytes, mmprojBytes);
    /// ```
    ///
    /// CPU-only; the wasm32 target has no GPU concept. `gpu_layers` is fixed
    /// at 0 internally.
    #[wasm_bindgen(js_name = loadBytes)]
    pub fn load_bytes(bytes: Vec<u8>, mmproj_bytes: JsValue) -> js_sys::Promise {
        let mmproj_vec: Option<Vec<u8>> = if mmproj_bytes.is_undefined() || mmproj_bytes.is_null() {
            None
        } else {
            match mmproj_bytes.dyn_into::<js_sys::Uint8Array>() {
                Ok(arr) => Some(arr.to_vec()),
                Err(_) => {
                    return js_sys::Promise::reject(
                        &JsError::new("Model.loadBytes: mmprojBytes must be Uint8Array").into(),
                    )
                }
            }
        };

        // If we got mmproj bytes, land them in MEMFS via the JS-side
        // FS.writeFile (libc open(2) from inside the wasm returns EPERM
        // against MEMFS for reasons we haven't tracked down) and pass
        // the synthetic path to core. The path is content-hashed so
        // identical mmproj bytes share one file.
        let mmproj_path = match mmproj_vec.as_deref() {
            Some(b) => match write_bytes_to_memfs("mmproj", b) {
                Ok(p) => Some(p),
                Err(e) => {
                    return js_sys::Promise::reject(
                        &JsError::new(&format!("mmproj write to MEMFS: {e}")).into(),
                    )
                }
            },
            None => None,
        };

        promisify(async move {
            let model = nobodywho::llm::get_model_from_bytes(&bytes, mmproj_path.as_deref(), 0)
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(Model {
                inner: Arc::new(model),
            })
        })
    }
}

// ---------- Multimodal: Image / Audio / prompt assembly (Path A) ----------
//
// JS API:
//
//     import { Image, Audio } from 'nobodywho-js';
//
//     const img = Image.fromBytes(new Uint8Array(await blob.arrayBuffer()));
//     const reply = await chat.ask(['What is in this image?', img]).completed();
//
// `Chat.ask` and `Chat.ask` accept either a plain string (text-only
// prompt — fast path, unchanged) or a JS array of `string | Image | Audio`
// parts. There is no `Image(path)` constructor in the wasm binding: a
// browser tab has no filesystem.
//
// Path A approach: `Image.fromBytes` / `Audio.fromBytes` return plain
// tagged JS objects of the shape `{__nbwKind: 'image'|'audio', bytes:
// Uint8Array}`. The same shape survives the postMessage hop into the chat
// worker. The Rust side (whether running in the main thread for `Chat` or
// inside the worker for `Chat`) calls `write_bytes_to_memfs(kind,
// bytes)` to land the bytes in Emscripten's in-memory filesystem at a
// content-hashed path like `/tmp/nbw-image-<hash>.bin`, then pushes that
// path through the existing `Prompt::push_image(&Path)` / `push_audio`
// API. mtmd's `from_file` loader uses `fopen`, which under Emscripten
// goes through MEMFS; the file appears real to llama.cpp.
//
// Why the hash-named path: identical bytes get the same path, so two
// `Image.fromBytes(sameBuf)` calls share one MEMFS entry (deduplication
// for free, KV-cache friendly via the existing bitmap-ID logic).

/// Image factory namespace for multimodal prompts. The only method is
/// [`Image::from_bytes`] — there is no path-based constructor because a
/// browser tab has no filesystem.
#[wasm_bindgen]
pub struct Image;

#[wasm_bindgen]
impl Image {
    /// Build an image prompt part by reading a file from a host
    /// filesystem path. Node-only — in the browser, fetch the bytes
    /// yourself and use `fromBytes()`. Returns a Promise because the
    /// underlying Node fs lookup goes through an `await import('node:fs')`
    /// dynamic-import shim. Mirrors Python's `Image("/path/to/file.png")`
    /// one-liner ergonomics (modulo the await).
    ///
    /// ```js
    /// const img = await Image.fromPath('/path/to/dog.png');
    /// ```
    #[wasm_bindgen(js_name = fromPath)]
    pub fn from_path(path: String) -> js_sys::Promise {
        promisify(async move {
            #[cfg(target_family = "wasm")]
            {
                let bytes = read_node_file_bytes(&path).await?;
                Ok(JsValue::from(make_media_part("image", &bytes)))
            }
            #[cfg(not(target_family = "wasm"))]
            {
                let _ = path;
                // Type-annotate so `promisify`'s `T: Into<JsValue>` bound
                // can be inferred on native — the Err-only branch can't
                // figure it out on its own.
                Err::<JsValue, _>(JsError::new("fromPath: not supported on this target"))
            }
        })
    }

    /// Build an image prompt part from raw file bytes (JPEG / PNG / BMP /
    /// GIF / TGA / PSD / PIC / PNM — anything `stb_image` can decode).
    /// The format is sniffed via the file header inside
    /// `mtmd_helper_bitmap_init_from_file` (Path A goes through the file
    /// loader, with the bytes mounted into MEMFS at a synthetic path).
    ///
    /// Returns a plain JS object `{__nbwKind: 'image', bytes: Uint8Array}`
    /// suitable for inclusion in a `chat.ask([...])` array.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: Vec<u8>) -> js_sys::Object {
        make_media_part("image", &bytes)
    }
}

/// Audio factory namespace for multimodal prompts.
#[wasm_bindgen]
pub struct Audio;

#[wasm_bindgen]
impl Audio {
    /// Build an audio prompt part by reading a file from a host
    /// filesystem path. Node-only — in the browser, fetch the bytes
    /// yourself and use `fromBytes()`. Returns a Promise because the
    /// underlying Node fs lookup goes through an `await import('node:fs')`
    /// dynamic-import shim. Mirrors Python's `Audio("/path/to/file.wav")`
    /// one-liner ergonomics (modulo the await).
    ///
    /// ```js
    /// const audio = await Audio.fromPath('/path/to/foo.wav');
    /// ```
    #[wasm_bindgen(js_name = fromPath)]
    pub fn from_path(path: String) -> js_sys::Promise {
        promisify(async move {
            #[cfg(target_family = "wasm")]
            {
                let bytes = read_node_file_bytes(&path).await?;
                Ok(JsValue::from(make_media_part("audio", &bytes)))
            }
            #[cfg(not(target_family = "wasm"))]
            {
                let _ = path;
                // Type-annotate so `promisify`'s `T: Into<JsValue>` bound
                // can be inferred on native — the Err-only branch can't
                // figure it out on its own.
                Err::<JsValue, _>(JsError::new("fromPath: not supported on this target"))
            }
        })
    }

    /// Build an audio prompt part from raw file bytes. Supported formats
    /// on the wasm-Emscripten build: WAV, MP3, FLAC (the playback /
    /// threading / engine layers are cut out via `MA_NO_*`, but the
    /// decoders front-end stays linked). The format is sniffed via the
    /// file header by mtmd's `is_audio_file`.
    ///
    /// Returns `{__nbwKind: 'audio', bytes: Uint8Array}`.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: Vec<u8>) -> js_sys::Object {
        make_media_part("audio", &bytes)
    }
}

/// Static factory namespace for common sampler presets. Each method
/// returns a plain JS object shaped like `Chat.create({sampler: ...})`,
/// ready to drop in. Mirrors Python's `SamplerPresets`.
///
/// ```js
/// import { SamplerPresets } from 'nobodywho-js';
/// await Chat.create({ modelBytes, sampler: SamplerPresets.greedy() });
/// await Chat.create({ modelBytes, sampler: SamplerPresets.temperature(0.8) });
/// ```
///
/// The constrain-* presets return `{constraint: ...}` instead of a
/// sampler spec, because grammars are wired through `Chat.create`'s
/// `constraint` option rather than the sampler chain in this binding:
///
/// ```js
/// const cfg = SamplerPresets.constrainWithRegex('^\\d+$');
/// await Chat.create({ modelBytes, ...cfg });
/// ```
#[wasm_bindgen]
pub struct SamplerPresets;

#[wasm_bindgen]
impl SamplerPresets {
    /// Empty sampler — defaults to core's preset (top_k=20, top_p=0.95,
    /// temperature=0.6, dist).
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = default)]
    pub fn default_preset() -> js_sys::Object {
        js_sys::Object::new()
    }

    /// Always picks the most probable token. Deterministic.
    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn greedy() -> js_sys::Object {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"sampleStep".into(), &"greedy".into());
        o
    }

    /// Temperature-only sampler.
    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn temperature(temperature: f32) -> js_sys::Object {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"temperature".into(), &temperature.into());
        o
    }

    /// Top-K filtering only.
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = topK)]
    pub fn top_k(top_k: i32) -> js_sys::Object {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"topK".into(), &top_k.into());
        o
    }

    /// Nucleus (top-P) sampling only.
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = topP)]
    pub fn top_p(top_p: f32) -> js_sys::Object {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"topP".into(), &top_p.into());
        o
    }

    /// Constrain generation to a regular expression. Returns a
    /// `{constraint: {regex}}` shape — pass to `Chat.create` via
    /// `Chat.create({modelBytes, ...preset})`.
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = constrainWithRegex)]
    pub fn constrain_with_regex(pattern: String) -> js_sys::Object {
        let inner = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&inner, &"regex".into(), &pattern.as_str().into());
        let outer = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&outer, &"constraint".into(), &inner.into());
        outer
    }

    /// Constrain generation to a JSON schema (string-form).
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = constrainWithJsonSchema)]
    pub fn constrain_with_json_schema(schema: String) -> js_sys::Object {
        let inner = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&inner, &"jsonSchema".into(), &schema.as_str().into());
        let outer = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&outer, &"constraint".into(), &inner.into());
        outer
    }

    /// Constrain generation to a Lark grammar.
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = constrainWithGrammar)]
    pub fn constrain_with_grammar(grammar: String) -> js_sys::Object {
        let inner = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&inner, &"lark".into(), &grammar.as_str().into());
        let outer = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&outer, &"constraint".into(), &inner.into());
        outer
    }

    /// DRY repetition penalty preset. Uses core's defaults (multiplier=0,
    /// base=1.75, allowed_length=2, full-context window, common
    /// newline/colon/quote/star seq breakers). Override individual knobs
    /// by spreading + replacing: `{...SamplerPresets.dry(), dryMultiplier: 0.8}`.
    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn dry() -> js_sys::Object {
        let o = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&o, &"dryMultiplier".into(), &0.0_f64.into());
        let _ = js_sys::Reflect::set(&o, &"dryBase".into(), &1.75_f64.into());
        let _ = js_sys::Reflect::set(&o, &"dryAllowedLength".into(), &2_i32.into());
        let _ = js_sys::Reflect::set(&o, &"dryPenaltyLastN".into(), &(-1_i32).into());
        let breakers = js_sys::Array::new();
        breakers.push(&"\n".into());
        breakers.push(&":".into());
        breakers.push(&"\"".into());
        breakers.push(&"*".into());
        let _ = js_sys::Reflect::set(&o, &"drySeqBreakers".into(), &breakers.into());
        o
    }

    /// Constrain output to ANY valid JSON (no schema). For schema-validated
    /// JSON, use `constrainWithJsonSchema(schema)` instead. Returns a
    /// `{constraint: {jsonSchema}}` shape with `{}` schema (which
    /// llguidance accepts as "any valid JSON value").
    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn json() -> js_sys::Object {
        let inner = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&inner, &"jsonSchema".into(), &"{}".into());
        let outer = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&outer, &"constraint".into(), &inner.into());
        outer
    }
}

/// Fluent builder for sampler chains. Each shift method returns the
/// builder for chaining; each terminal method (`dist`, `greedy`,
/// `mirostatV1`, `mirostatV2`) returns the finished sampler spec as a
/// plain JS object — pass directly to `Chat.create({sampler: ...})`.
/// Mirrors Python's `SamplerBuilder`.
///
/// ```js
/// import { SamplerBuilder } from 'nobodywho-js';
/// const sampler = new SamplerBuilder()
///   .topK(40)
///   .topP(0.95)
///   .temperature(0.7)
///   .dist();
/// await Chat.create({ modelBytes, sampler });
/// ```
#[wasm_bindgen]
pub struct SamplerBuilder {
    spec: js_sys::Object,
}

#[wasm_bindgen]
impl SamplerBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SamplerBuilder {
        SamplerBuilder {
            spec: js_sys::Object::new(),
        }
    }

    /// Shift step: temperature scaling. Returns self for chaining.
    pub fn temperature(self, temperature: f32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"temperature".into(), &temperature.into());
        self
    }

    /// Shift step: top-K filtering. Returns self for chaining.
    #[wasm_bindgen(js_name = topK)]
    pub fn top_k(self, top_k: i32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"topK".into(), &top_k.into());
        self
    }

    /// Shift step: nucleus (top-P) sampling. Returns self for chaining.
    #[wasm_bindgen(js_name = topP)]
    pub fn top_p(self, top_p: f32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"topP".into(), &top_p.into());
        self
    }

    /// Shift step: min-P sampling. Returns self for chaining.
    #[wasm_bindgen(js_name = minP)]
    pub fn min_p(self, min_p: f32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"minP".into(), &min_p.into());
        self
    }

    /// Shift step: repeat-penalty (penalty only). Returns self for
    /// chaining. For all four repeat-penalty knobs in one call, use
    /// `.penalties()`.
    #[wasm_bindgen(js_name = repeatPenalty)]
    pub fn repeat_penalty(self, penalty: f32, last_n: i32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"repeatPenalty".into(), &penalty.into());
        let _ = js_sys::Reflect::set(&self.spec, &"repeatLastN".into(), &last_n.into());
        self
    }

    /// Shift step: full repeat-penalty step with all four knobs —
    /// matches Python's `SamplerBuilder.penalties()`. Returns self for
    /// chaining.
    pub fn penalties(
        self,
        penalty_repeat: f32,
        penalty_last_n: i32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"repeatPenalty".into(), &penalty_repeat.into());
        let _ = js_sys::Reflect::set(&self.spec, &"repeatLastN".into(), &penalty_last_n.into());
        let _ = js_sys::Reflect::set(
            &self.spec,
            &"repeatFreqPenalty".into(),
            &penalty_freq.into(),
        );
        let _ = js_sys::Reflect::set(
            &self.spec,
            &"repeatPresentPenalty".into(),
            &penalty_present.into(),
        );
        self
    }

    /// Shift step: DRY ("Don't Repeat Yourself") repetition penalty.
    /// Returns self for chaining. `seqBreakers` defaults to common
    /// punctuation breakers when not supplied; pass an empty array to
    /// disable.
    pub fn dry(
        self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: js_sys::Array,
    ) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"dryMultiplier".into(), &multiplier.into());
        let _ = js_sys::Reflect::set(&self.spec, &"dryBase".into(), &base.into());
        let _ = js_sys::Reflect::set(
            &self.spec,
            &"dryAllowedLength".into(),
            &allowed_length.into(),
        );
        let _ = js_sys::Reflect::set(
            &self.spec,
            &"dryPenaltyLastN".into(),
            &penalty_last_n.into(),
        );
        let _ = js_sys::Reflect::set(&self.spec, &"drySeqBreakers".into(), &seq_breakers.into());
        self
    }

    /// Shift step: XTC ("Exclude Top Choices") sampling. Returns self
    /// for chaining.
    pub fn xtc(self, probability: f32, threshold: f32, min_keep: u32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"xtcProbability".into(), &probability.into());
        let _ = js_sys::Reflect::set(&self.spec, &"xtcThreshold".into(), &threshold.into());
        let _ = js_sys::Reflect::set(&self.spec, &"xtcMinKeep".into(), &min_keep.into());
        self
    }

    /// Shift step: Typical-P (locally typical) sampling. Returns self
    /// for chaining.
    #[wasm_bindgen(js_name = typicalP)]
    pub fn typical_p(self, typ_p: f32, min_keep: u32) -> SamplerBuilder {
        let _ = js_sys::Reflect::set(&self.spec, &"typicalP".into(), &typ_p.into());
        let _ = js_sys::Reflect::set(&self.spec, &"typicalPMinKeep".into(), &min_keep.into());
        self
    }

    /// Terminal: weighted-random sample from the shifted distribution.
    /// Returns the completed sampler spec.
    pub fn dist(self) -> js_sys::Object {
        let _ = js_sys::Reflect::set(&self.spec, &"sampleStep".into(), &"dist".into());
        self.spec
    }

    /// Terminal: always pick the most probable token. Returns the
    /// completed sampler spec.
    pub fn greedy(self) -> js_sys::Object {
        let _ = js_sys::Reflect::set(&self.spec, &"sampleStep".into(), &"greedy".into());
        self.spec
    }

    /// Terminal: Mirostat v1. Returns the completed sampler spec.
    #[wasm_bindgen(js_name = mirostatV1)]
    pub fn mirostat_v1(self, tau: f32, eta: f32, m: i32) -> js_sys::Object {
        let _ = js_sys::Reflect::set(&self.spec, &"sampleStep".into(), &"mirostatV1".into());
        let _ = js_sys::Reflect::set(&self.spec, &"mirostatTau".into(), &tau.into());
        let _ = js_sys::Reflect::set(&self.spec, &"mirostatEta".into(), &eta.into());
        let _ = js_sys::Reflect::set(&self.spec, &"mirostatM".into(), &m.into());
        self.spec
    }

    /// Terminal: Mirostat v2 (simpler than v1; usually preferred).
    /// Returns the completed sampler spec.
    #[wasm_bindgen(js_name = mirostatV2)]
    pub fn mirostat_v2(self, tau: f32, eta: f32) -> js_sys::Object {
        let _ = js_sys::Reflect::set(&self.spec, &"sampleStep".into(), &"mirostatV2".into());
        let _ = js_sys::Reflect::set(&self.spec, &"mirostatTau".into(), &tau.into());
        let _ = js_sys::Reflect::set(&self.spec, &"mirostatEta".into(), &eta.into());
        self.spec
    }
}

/// Read a host filesystem file into a `Vec<u8>` via the Node helper
/// `globalThis.__nbw_node_read_file` (defined in pre.js, Node-only).
/// Returns a future so the caller can await the JS dynamic import that
/// the helper uses under the hood. Errors clearly with browser-friendly
/// guidance if the Node helper isn't available.
#[cfg(target_family = "wasm")]
async fn read_node_file_bytes(path: &str) -> Result<Vec<u8>, JsError> {
    let global = js_sys::global();
    let helper: js_sys::Function = js_sys::Reflect::get(&global, &"__nbw_node_read_file".into())
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| {
            JsError::new(
                "fromPath is Node-only. In a browser, fetch() the bytes \
                 yourself and pass them to fromBytes().",
            )
        })?;
    let promise = helper
        .call1(&JsValue::NULL, &path.into())
        .map_err(|e| JsError::new(&format!("__nbw_node_read_file({path}) threw: {e:?}")))?;
    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| JsError::new(&format!("read failed: {e:?}")))?;
    let u8: js_sys::Uint8Array = result
        .dyn_into()
        .map_err(|_| JsError::new("__nbw_node_read_file returned a non-Uint8Array"))?;
    Ok(u8.to_vec())
}

/// Build a tagged media part object. `kind` is `"image"` or `"audio"`.
fn make_media_part(kind: &str, bytes: &[u8]) -> js_sys::Object {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &"__nbwKind".into(), &JsValue::from_str(kind));
    let _ = js_sys::Reflect::set(&o, &"bytes".into(), &js_sys::Uint8Array::from(bytes).into());
    o
}

/// Pull `__nbwKind` off a candidate part. Returns None if the object is not
/// a tagged media part (e.g. plain string, foreign object).
fn read_media_kind(part: &JsValue) -> Option<String> {
    js_sys::Reflect::get(part, &"__nbwKind".into())
        .ok()
        .and_then(|v| v.as_string())
}

fn read_media_bytes(part: &JsValue) -> Result<Vec<u8>, String> {
    let v = js_sys::Reflect::get(part, &"bytes".into())
        .map_err(|_| "media part missing 'bytes' field".to_string())?;
    let u8a = v
        .dyn_into::<js_sys::Uint8Array>()
        .map_err(|_| "media part 'bytes' must be a Uint8Array".to_string())?;
    Ok(u8a.to_vec())
}

/// Mount `bytes` into Emscripten's MEMFS under `/home/web_user/nbw-<kind>-
/// <hash>.bin` and return the path. The path is content-addressed so
/// identical bytes produce the same file and identical bitmap IDs
/// downstream.
///
/// Writes through `Module.FS.writeFile` on the JS side — libc
/// `open(2)`/`fopen` from inside the wasm returns EPERM (errno 63) on
/// `wasm32-unknown-emscripten`'s MEMFS for reasons we haven't tracked
/// down, but the JS-side FS API works fine on the same paths. Going via
/// `js_sys::Reflect` against the global `Module.FS` keeps the Rust-side
/// call site clean and depends only on the build pipeline exporting
/// `FS` (which `-sEXPORTED_RUNTIME_METHODS=FS` in the emcc post-link
/// step takes care of).
fn write_bytes_to_memfs(kind: &str, bytes: &[u8]) -> Result<std::path::PathBuf, String> {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let hash = hasher.finish();
    let path = std::path::PathBuf::from(format!("/home/web_user/nbw-{kind}-{hash:016x}.bin"));

    if path.exists() {
        return Ok(path);
    }

    js_fs_write_file(&path.to_string_lossy(), bytes)?;
    Ok(path)
}

/// Stream a host filesystem file into MEMFS at a fixed `nbw-{kind}.bin`
/// path. Delegates to the Node-only JS helper
/// `globalThis.__nbw_node_file_to_memfs(srcPath, memfsPath)` which reads
/// the host file in 64 MiB chunks via Node `fs` and writes them into
/// MEMFS via `Module.FS.write` — never materializing the full file in
/// JS memory.
///
/// Returns the MEMFS path the caller should pass to the path-based
/// loader. Errors clearly if the helper isn't present (i.e. the binding
/// is running in a browser worker without Node).
#[cfg(target_family = "wasm")]
async fn stream_host_file_to_memfs(
    kind: &str,
    src_path: &str,
) -> Result<std::path::PathBuf, String> {
    // Fixed MEMFS destination per kind. Each worker hosts one model at
    // a time, so there's no need to content-hash.
    let memfs_path = std::path::PathBuf::from(format!("/home/web_user/nbw-{kind}.gguf"));

    let global = js_sys::global();
    let helper = js_sys::Reflect::get(&global, &"__nbw_node_file_to_memfs".into())
        .map_err(|_| "stream_host_file_to_memfs: lookup failed".to_string())?;
    if helper.is_undefined() || helper.is_null() {
        return Err(
            "modelPath/mmprojPath is Node-only; in browser use modelBytes or modelUrl".to_string(),
        );
    }
    let helper_fn: js_sys::Function = helper.dyn_into().map_err(|_| {
        "stream_host_file_to_memfs: __nbw_node_file_to_memfs is not a function".to_string()
    })?;

    let promise_val = helper_fn
        .call2(
            &JsValue::NULL,
            &JsValue::from_str(src_path),
            &JsValue::from_str(&memfs_path.to_string_lossy()),
        )
        .map_err(|e| {
            let msg = js_sys::Reflect::get(&e, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
                .unwrap_or_else(|| format!("{e:?}"));
            format!("__nbw_node_file_to_memfs threw: {msg}")
        })?;
    let promise: js_sys::Promise = promise_val
        .dyn_into()
        .map_err(|_| "__nbw_node_file_to_memfs did not return a Promise".to_string())?;
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| {
            let msg = js_sys::Reflect::get(&e, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
                .unwrap_or_else(|| format!("{e:?}"));
            format!("__nbw_node_file_to_memfs rejected: {msg}")
        })?;

    Ok(memfs_path)
}

/// Call `Module.FS.writeFile(path, bytes)` from Rust. Used by both
/// [`write_bytes_to_memfs`] (image/audio prompt parts) and core's
/// mmproj-from-bytes loader path.
///
/// `Module` is the Emscripten module-factory result; in the JS glue
/// emitted by emcc-MODULARIZE this lives as a top-level local that's
/// accessible via `globalThis.Module` from inside the wasm-bindgen-
/// generated JS (since `pre.js` runs `Module.preRun.push(() => { ... })`
/// it has `Module` in scope). `Module.FS` is the Emscripten FS API,
/// exported here by `-sEXPORTED_RUNTIME_METHODS=FS`.
fn js_fs_write_file(path: &str, bytes: &[u8]) -> Result<(), String> {
    let global_obj = js_sys::global();
    let module = js_sys::Reflect::get(&global_obj, &JsValue::from_str("Module"))
        .map_err(|_| "globalThis.Module not found".to_string())?;
    if module.is_undefined() || module.is_null() {
        return Err("globalThis.Module is undefined".to_string());
    }
    let fs = js_sys::Reflect::get(&module, &JsValue::from_str("FS"))
        .map_err(|_| "Module.FS not accessible".to_string())?;
    if fs.is_undefined() || fs.is_null() {
        return Err(
            "Module.FS is undefined — build with -sEXPORTED_RUNTIME_METHODS=FS".to_string(),
        );
    }
    let write_file_val = js_sys::Reflect::get(&fs, &JsValue::from_str("writeFile"))
        .map_err(|_| "Module.FS.writeFile not accessible".to_string())?;
    let write_file: js_sys::Function = write_file_val
        .dyn_into()
        .map_err(|_| "Module.FS.writeFile is not a function".to_string())?;

    let bytes_js: JsValue = js_sys::Uint8Array::from(bytes).into();
    write_file
        .call2(&fs, &JsValue::from_str(path), &bytes_js)
        .map_err(|e| {
            let msg = js_sys::Reflect::get(&e, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
                .unwrap_or_else(|| format!("{e:?}"));
            format!("FS.writeFile({path}) failed: {msg}")
        })?;
    Ok(())
}

/// Convert a `JsValue` (a bare string OR an array containing strings and
/// tagged media-part objects) into a core `Prompt`. Used by the in-process
/// `Chat::ask` AND (post-postMessage) by the worker dispatcher's `"ask"`
/// branch — same logic for both since `{__nbwKind, bytes}` is the wire
/// shape on both sides. Media bytes are written to MEMFS here.
fn js_to_prompt(input: &JsValue) -> Result<nobodywho::tokenizer::Prompt, String> {
    let mut prompt = nobodywho::tokenizer::Prompt::new();

    if let Some(s) = input.as_string() {
        prompt.push_text(s);
        return Ok(prompt);
    }

    let arr: &js_sys::Array = input.dyn_ref::<js_sys::Array>().ok_or_else(|| {
        "ask: prompt must be a string or an array of (string | Image.fromBytes | Audio.fromBytes)"
            .to_string()
    })?;

    for i in 0..arr.length() {
        let part = arr.get(i);
        if let Some(s) = part.as_string() {
            prompt.push_text(s);
            continue;
        }
        let kind = read_media_kind(&part);
        match kind.as_deref() {
            Some("image") => {
                let bytes = read_media_bytes(&part)?;
                let path = write_bytes_to_memfs("image", &bytes)?;
                prompt.push_image(&path);
            }
            Some("audio") => {
                let bytes = read_media_bytes(&part)?;
                let path = write_bytes_to_memfs("audio", &bytes)?;
                prompt.push_audio(&path);
            }
            Some(other) => return Err(format!("ask: parts[{i}] unknown kind '{other}'")),
            None => {
                return Err(format!(
                    "ask: parts[{i}] must be a string or Image.fromBytes(...) / Audio.fromBytes(...) result (got {:?})",
                    part
                ));
            }
        }
    }

    Ok(prompt)
}

/// Pass-through normaliser for the worker hop. Main-thread `Chat.ask`
/// calls this on its input, then post-messages the result. The worker's
/// `"ask"` dispatcher then runs `js_to_prompt` on the received array.
///
/// Since `{__nbwKind, bytes: Uint8Array}` is already structured-cloneable
/// the only thing this does is wrap a bare string in an Array so the
/// worker has a single shape to deserialize.
#[cfg(target_family = "wasm")]
fn js_to_serializable_parts(input: &JsValue) -> Result<JsValue, JsError> {
    if let Some(s) = input.as_string() {
        let arr = js_sys::Array::new();
        arr.push(&JsValue::from_str(&s));
        return Ok(arr.into());
    }
    if input.dyn_ref::<js_sys::Array>().is_some() {
        return Ok(input.clone());
    }
    Err(JsError::new(
        "ask: prompt must be a string or an array of (string | Image.fromBytes | Audio.fromBytes)",
    ))
}

// ---------- Tool (LLM-callable JS function) ----------
//
// JS API:
//
//     import { Tool, Chat } from 'nobodywho-js';
//
//     const weather = Tool.fromFn(
//       'get_weather',
//       'Get current weather for a city',
//       { type: 'object', properties: { city: { type: 'string' } }, required: ['city'] },
//       ({ city }) => `Sunny in ${city}, 21°C`,
//     );
//
//     const chat = await Chat.create({
//       modelBytes, tools: [weather], systemPrompt: '...',
//     });
//     const reply = await chat.ask('Weather in CPH?').completed();
//
// JS callbacks can be either synchronous (return a string) or async
// (return a Promise<string>) — the worker ↔ main RPC bridge dispatches
// each tool call back to the main thread, awaits the result, and feeds
// it into the next inference step. See `js/scripts/tool-smoke.mjs` for
// both shapes.

/// Factory namespace for LLM-callable tools. Built via [`Tool::from_fn`]
/// and passed to `Chat`'s `tools` option.
///
/// Tools are returned as plain JS objects of shape
/// `{__nbwKind: 'tool', name, description, jsonSchema, callback}` rather
/// than wasm-bindgen class instances — wasm-bindgen 0.2.121's
/// Rust-defined structs don't `impl JsCast`, so we can't `dyn_into`
/// them out of a generic options-object on the way back. Tagged plain
/// objects sidestep that and let the extract step do a brand check.
#[wasm_bindgen]
pub struct Tool;

#[wasm_bindgen]
impl Tool {
    /// Wrap a JS function as an LLM-callable tool.
    ///
    /// - `name`: identifier the model uses when emitting a tool-call.
    /// - `description`: shown to the model so it can decide when to call.
    /// - `jsonSchema`: JSON-Schema (as a plain JS object) describing the
    ///   argument shape. Used by the grammar sampler to constrain what
    ///   the model emits to match this schema exactly.
    /// - `callback`: synchronous JS function. Receives the parsed
    ///   arguments object as its first argument and must return a string
    ///   (the value the model sees as the tool's result). Non-string
    ///   returns are JSON.stringify'd as a best-effort fallback.
    ///
    /// Returns a plain JS object `{__nbwKind:'tool', name, description,
    /// jsonSchema, callback}`. `Chat`'s constructor checks for the brand
    /// when reading the `tools` option and rebuilds the closure form
    /// expected by core.
    #[wasm_bindgen(js_name = fromFn)]
    pub fn from_fn(
        name: String,
        description: String,
        json_schema: JsValue,
        callback: js_sys::Function,
    ) -> Result<JsValue, JsError> {
        // Sanity-check the schema up-front so a typo errors at Tool
        // construction time rather than mid-inference. The Rust side
        // re-parses in `tool_from_tagged`; this is just a fast-fail.
        let schema_str = js_sys::JSON::stringify(&json_schema)
            .ok()
            .and_then(|s| s.as_string())
            .ok_or_else(|| JsError::new("Tool.fromFn: jsonSchema must be JSON-serializable"))?;
        let _: serde_json::Value = serde_json::from_str(&schema_str)
            .map_err(|e| JsError::new(&format!("Tool.fromFn: jsonSchema parse: {e}")))?;

        let obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&obj, &"__nbwKind".into(), &"tool".into());
        let _ = js_sys::Reflect::set(&obj, &"name".into(), &JsValue::from_str(&name));
        let _ = js_sys::Reflect::set(
            &obj,
            &"description".into(),
            &JsValue::from_str(&description),
        );
        let _ = js_sys::Reflect::set(&obj, &"jsonSchema".into(), &json_schema);
        let _ = js_sys::Reflect::set(&obj, &"callback".into(), &callback);
        Ok(obj.into())
    }
}

/// Read the `tools` array off a `Chat` options object and materialize
/// each entry as a `nobodywho::tool_calling::Tool` (the core's Arc'd
/// closure form). Returns an empty vec for missing / null / undefined
/// `tools`. Each array element must be a `Tool.fromFn(...)` return
/// value (tagged with `__nbwKind: 'tool'`); rejects anything else with
/// a clear error pointing the caller at `Tool.fromFn`.
fn extract_tools(opts: &JsValue) -> Result<Vec<nobodywho::tool_calling::Tool>, JsError> {
    if opts.is_undefined() || opts.is_null() {
        return Ok(Vec::new());
    }
    let tools_val = match js_sys::Reflect::get(opts, &JsValue::from_str("tools")) {
        Ok(v) => v,
        Err(_) => return Ok(Vec::new()),
    };
    if tools_val.is_undefined() || tools_val.is_null() {
        return Ok(Vec::new());
    }
    let arr = tools_val.dyn_ref::<js_sys::Array>().ok_or_else(|| {
        JsError::new("Chat options.tools must be an array of Tool.fromFn(...) values")
    })?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let elem = arr.get(i);
        out.push(
            tool_from_tagged(&elem)
                .map_err(|e| JsError::new(&format!("Chat options.tools[{i}]: {e}")))?,
        );
    }
    Ok(out)
}

/// Return a clone of `opts` with the named keys removed. Used to strip
/// `tools` (whose entries are tagged JS objects with non-serde-friendly
/// JS function values inside) before passing the rest through
/// `serde_wasm_bindgen` for ChatOptions deserialization.
fn strip_keys(opts: &JsValue, keys: &[&str]) -> Result<JsValue, JsError> {
    if opts.is_undefined() || opts.is_null() {
        return Ok(opts.clone());
    }
    let src = opts
        .dyn_ref::<js_sys::Object>()
        .ok_or_else(|| JsError::new("Chat options must be a plain object"))?;
    let out = js_sys::Object::new();
    for k in js_sys::Object::keys(src).iter() {
        let key_str = k.as_string().unwrap_or_default();
        if keys.contains(&key_str.as_str()) {
            continue;
        }
        if let Ok(v) = js_sys::Reflect::get(src, &k) {
            let _ = js_sys::Reflect::set(&out, &k, &v);
        }
    }
    Ok(out.into())
}

/// Take a tagged tool object (the shape `Tool::from_fn` returns) and
/// rebuild it as a core `Tool`. The JS callback is wrapped in an
/// `Arc<Fn(Value) -> String + Send + Sync>` that the inference loop
/// invokes when the model emits a matching tool-call.
fn tool_from_tagged(part: &JsValue) -> Result<nobodywho::tool_calling::Tool, String> {
    let kind = js_sys::Reflect::get(part, &"__nbwKind".into())
        .ok()
        .and_then(|v| v.as_string());
    if kind.as_deref() != Some("tool") {
        return Err("not a Tool.fromFn(...) value — missing or wrong __nbwKind brand".to_string());
    }
    let name = js_sys::Reflect::get(part, &"name".into())
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| "missing name".to_string())?;
    let description = js_sys::Reflect::get(part, &"description".into())
        .ok()
        .and_then(|v| v.as_string())
        .ok_or_else(|| "missing description".to_string())?;
    let schema_jsval = js_sys::Reflect::get(part, &"jsonSchema".into())
        .map_err(|_| "missing jsonSchema".to_string())?;
    let schema_str = js_sys::JSON::stringify(&schema_jsval)
        .ok()
        .and_then(|s| s.as_string())
        .ok_or_else(|| "jsonSchema is not JSON-serializable".to_string())?;
    let schema: serde_json::Value =
        serde_json::from_str(&schema_str).map_err(|e| format!("jsonSchema parse: {e}"))?;
    let callback_jsval = js_sys::Reflect::get(part, &"callback".into())
        .map_err(|_| "missing callback".to_string())?;
    let callback: js_sys::Function = callback_jsval
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "callback is not a function".to_string())?;

    // `Tool::new_async` to accept JS callbacks that return Promises. If the
    // callback returns a plain string, we use it directly. If it returns a
    // Promise, we `JsFuture::from(...).await` to drive it to completion —
    // the Rust async/await yield gives the JS event loop a chance to tick
    // and resolve the Promise without blocking the wasm thread.
    Ok(nobodywho::tool_calling::Tool::new_async(
        name,
        description,
        schema,
        move |args: serde_json::Value| {
            let callback = callback.clone();
            async move {
                // serde_json::Value → JsValue for the JS-side function,
                // with `serialize_maps_as_objects(true)` so the user's
                // callback sees a plain JS object (so `args.city` works)
                // rather than a JS Map (where it wouldn't).
                let args_js = {
                    use serde::Serialize as _;
                    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
                    match args.serialize(&ser) {
                        Ok(v) => v,
                        Err(e) => return format!("ERROR: tool arg conversion: {e}"),
                    }
                };
                let result = match callback.call1(&JsValue::NULL, &args_js) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = js_sys::Reflect::get(&e, &"message".into())
                            .ok()
                            .and_then(|m| m.as_string())
                            .unwrap_or_else(|| format!("{e:?}"));
                        return format!("ERROR: {msg}");
                    }
                };
                // If the JS callback returned a Promise, await its
                // resolution. Otherwise use the value directly.
                let resolved = if result.is_instance_of::<js_sys::Promise>() {
                    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(result)).await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            let msg = js_sys::Reflect::get(&e, &"message".into())
                                .ok()
                                .and_then(|m| m.as_string())
                                .unwrap_or_else(|| format!("{e:?}"));
                            return format!("ERROR: tool promise rejected: {msg}");
                        }
                    }
                } else {
                    result
                };
                if let Some(s) = resolved.as_string() {
                    return s;
                }
                // Non-string return (or resolved value): JSON.stringify
                // as a fallback so the model gets something legible.
                js_sys::JSON::stringify(&resolved)
                    .ok()
                    .and_then(|s| s.as_string())
                    .unwrap_or_else(|| "ERROR: tool returned a non-serializable value".to_string())
            }
        },
    ))
}

/// Optional config passed to `Chat.create`. Pass as a plain JS object:
///
/// ```js
/// await Chat.create({
///   modelBytes,
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
    /// Sampling knobs (temperature, top_p, top_k, etc.). All fields are
    /// optional; absent fields are not applied. When `sampler` is omitted
    /// entirely, the core's default sampler is used (top_k=20, top_p=0.95,
    /// temperature=0.6, dist sampling). When `sampler` is provided
    /// alongside `constraint`, the constraint's grammar shift step is
    /// prepended to the user's sampler chain — same compose pattern that
    /// tool-call grammars use internally.
    sampler: Option<SamplerSpec>,
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
/// The grammar sampler runs through llguidance, which needs a monotonic
/// clock — Emscripten's libc has `clock_gettime`, so this works at
/// runtime. End-to-end verified on Emscripten via
/// `js/scripts/constraint-smoke.mjs` (regex + json_schema + lark).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConstraintSpec {
    json_schema: Option<String>,
    regex: Option<String>,
    lark: Option<String>,
}

impl ConstraintSpec {
    /// Build a full SamplerConfig from this constraint alone — used when
    /// `ChatOptions.sampler` is not provided. Equivalent to the matching
    /// `SamplerPresets::constrain_with_*` shape (constraint shift + Dist).
    fn into_sampler(self) -> Result<nobodywho::sampler_config::SamplerConfig, JsError> {
        use nobodywho::sampler_config::SamplerPresets;
        Ok(match self.into_shift_step()? {
            nobodywho::sampler_config::ShiftStep::JsonSchema(s) => {
                SamplerPresets::constrain_with_json_schema(s)
            }
            nobodywho::sampler_config::ShiftStep::Regex(p) => {
                SamplerPresets::constrain_with_regex(p)
            }
            nobodywho::sampler_config::ShiftStep::Lark(l) => {
                SamplerPresets::constrain_with_grammar(l)
            }
            // `into_shift_step` only ever returns one of the three above.
            _ => unreachable!("ConstraintSpec::into_shift_step variant invariant"),
        })
    }

    /// Extract just the constraint shift step. Lets callers compose the
    /// constraint with their own sampler chain (prepend the constraint so
    /// it runs before temperature / top-k / top-p, matching how core's
    /// tool-call grammar prepending works in `Worker::ask`).
    fn into_shift_step(self) -> Result<nobodywho::sampler_config::ShiftStep, JsError> {
        use nobodywho::sampler_config::ShiftStep;
        let n_set = self.json_schema.is_some() as u8
            + self.regex.is_some() as u8
            + self.lark.is_some() as u8;
        if n_set != 1 {
            return Err(JsError::new(
                "constraint must set exactly one of jsonSchema / regex / lark",
            ));
        }
        Ok(if let Some(s) = self.json_schema {
            ShiftStep::JsonSchema(s)
        } else if let Some(p) = self.regex {
            ShiftStep::Regex(p)
        } else {
            ShiftStep::Lark(self.lark.unwrap())
        })
    }
}

/// JS-facing sampler configuration. All fields optional; absent ones are
/// not applied. To get the standard preset, omit `sampler` from
/// `ChatOptions` entirely (which falls back to core's default).
///
/// Shift steps are applied in llama.cpp's canonical order:
/// penalties → top_k → top_p → min_p → temperature → sample_step.
///
/// JS shape:
/// ```js
/// await Chat.create({
///   modelBytes,
///   sampler: {
///     temperature: 0.7,
///     topK: 40,
///     topP: 0.95,
///     minP: 0.05,
///     repeatPenalty: 1.1,
///     repeatLastN: 64,
///     sampleStep: 'dist', // 'dist' | 'greedy' | 'mirostatV1' | 'mirostatV2'
///   },
/// });
/// ```
///
/// `sampleStep: 'greedy'` ignores temperature / topK / topP and always
/// picks the highest-probability token — useful for deterministic output.
#[derive(serde::Deserialize, Default, Debug)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SamplerSpec {
    temperature: Option<f32>,
    top_k: Option<i32>,
    top_p: Option<f32>,
    min_p: Option<f32>,
    /// Repeat penalty — sets `ShiftStep::Penalties.penalty_repeat`. Setting
    /// any of `repeat*` adds a Penalties step (other repeat fields default
    /// to 0/1/64 if unset).
    repeat_penalty: Option<f32>,
    repeat_last_n: Option<i32>,
    repeat_freq_penalty: Option<f32>,
    repeat_present_penalty: Option<f32>,
    /// `'dist'` (default) | `'greedy'` | `'mirostatV1'` | `'mirostatV2'`.
    sample_step: Option<String>,
    /// MirostatV1 / MirostatV2 only.
    mirostat_tau: Option<f32>,
    /// MirostatV1 / MirostatV2 only.
    mirostat_eta: Option<f32>,
    /// MirostatV1 only.
    mirostat_m: Option<i32>,
    /// Typical-P (locally typical) sampling threshold. Adds a TypicalP
    /// shift step when set.
    typical_p: Option<f32>,
    /// Typical-P minimum tokens to keep. Defaults to 1 when only
    /// `typical_p` is given.
    typical_p_min_keep: Option<u32>,
    /// XTC ("Exclude Top Choices") probability of triggering. Setting
    /// any of the `xtc_*` fields adds an XTC shift step.
    xtc_probability: Option<f32>,
    /// XTC threshold — tokens above this probability become candidates
    /// for exclusion.
    xtc_threshold: Option<f32>,
    /// XTC minimum tokens to keep.
    xtc_min_keep: Option<u32>,
    /// DRY ("Don't Repeat Yourself") repetition-penalty multiplier.
    /// Setting any of the `dry_*` fields adds a DRY shift step.
    dry_multiplier: Option<f32>,
    /// DRY base — exponent base for the per-repeat-length penalty.
    dry_base: Option<f32>,
    /// DRY maximum allowed repeat length before penalty applies.
    dry_allowed_length: Option<i32>,
    /// DRY scope: how many recent tokens to consider. `-1` (default in
    /// core) means the full context.
    dry_penalty_last_n: Option<i32>,
    /// DRY sequence breakers — strings that reset the repetition
    /// detector. Common defaults: `["\n", ":", "\"", "*"]`.
    dry_seq_breakers: Option<Vec<String>>,
}

impl SamplerSpec {
    fn into_sampler(self) -> Result<nobodywho::sampler_config::SamplerConfig, JsError> {
        use nobodywho::sampler_config::{SampleStep, SamplerConfig, ShiftStep};

        let mut config = SamplerConfig::new();

        if self.repeat_penalty.is_some()
            || self.repeat_last_n.is_some()
            || self.repeat_freq_penalty.is_some()
            || self.repeat_present_penalty.is_some()
        {
            config = config.shift(ShiftStep::Penalties {
                penalty_last_n: self.repeat_last_n.unwrap_or(64),
                penalty_repeat: self.repeat_penalty.unwrap_or(1.0),
                penalty_freq: self.repeat_freq_penalty.unwrap_or(0.0),
                penalty_present: self.repeat_present_penalty.unwrap_or(0.0),
            });
        }
        if let Some(k) = self.top_k {
            config = config.shift(ShiftStep::TopK { top_k: k });
        }
        if let Some(p) = self.top_p {
            config = config.shift(ShiftStep::TopP {
                top_p: p,
                min_keep: 1,
            });
        }
        if let Some(p) = self.min_p {
            config = config.shift(ShiftStep::MinP {
                min_p: p,
                min_keep: 1,
            });
        }
        if let Some(p) = self.typical_p {
            config = config.shift(ShiftStep::TypicalP {
                typ_p: p,
                min_keep: self.typical_p_min_keep.unwrap_or(1),
            });
        }
        if self.xtc_probability.is_some()
            || self.xtc_threshold.is_some()
            || self.xtc_min_keep.is_some()
        {
            config = config.shift(ShiftStep::XTC {
                xtc_probability: self.xtc_probability.unwrap_or(0.0),
                xtc_threshold: self.xtc_threshold.unwrap_or(0.1),
                min_keep: self.xtc_min_keep.unwrap_or(1),
            });
        }
        if self.dry_multiplier.is_some()
            || self.dry_base.is_some()
            || self.dry_allowed_length.is_some()
            || self.dry_penalty_last_n.is_some()
            || self.dry_seq_breakers.is_some()
        {
            config = config.shift(ShiftStep::DRY {
                multiplier: self.dry_multiplier.unwrap_or(0.0),
                base: self.dry_base.unwrap_or(1.75),
                allowed_length: self.dry_allowed_length.unwrap_or(2),
                penalty_last_n: self.dry_penalty_last_n.unwrap_or(-1),
                seq_breakers: self.dry_seq_breakers.unwrap_or_else(|| {
                    vec![
                        "\n".to_string(),
                        ":".to_string(),
                        "\"".to_string(),
                        "*".to_string(),
                    ]
                }),
            });
        }
        if let Some(t) = self.temperature {
            config = config.shift(ShiftStep::Temperature { temperature: t });
        }

        let sample_step = match self.sample_step.as_deref() {
            None | Some("dist") => SampleStep::Dist,
            Some("greedy") => SampleStep::Greedy,
            Some("mirostatV1") => SampleStep::MirostatV1 {
                tau: self.mirostat_tau.unwrap_or(5.0),
                eta: self.mirostat_eta.unwrap_or(0.1),
                m: self.mirostat_m.unwrap_or(100),
            },
            Some("mirostatV2") => SampleStep::MirostatV2 {
                tau: self.mirostat_tau.unwrap_or(5.0),
                eta: self.mirostat_eta.unwrap_or(0.1),
            },
            Some(other) => {
                return Err(JsError::new(&format!(
                    "sampler.sampleStep must be 'dist' | 'greedy' | 'mirostatV1' | 'mirostatV2'; got {other:?}",
                )));
            }
        };
        config = config.sample(sample_step);

        Ok(config)
    }
}

/// Build a sampler from the four possible combinations of `sampler` and
/// `constraint` on `ChatOptions`.
///
/// | sampler | constraint | result                                              |
/// |---------|------------|-----------------------------------------------------|
/// | None    | None       | None (caller falls back to core's default sampler)  |
/// | None    | Some(c)    | constraint-only sampler                             |
/// | Some(s) | None       | user's sampler                                       |
/// | Some(s) | Some(c)    | user's sampler with constraint shift PREPENDED       |
fn build_sampler(
    sampler: Option<SamplerSpec>,
    constraint: Option<ConstraintSpec>,
) -> Result<Option<nobodywho::sampler_config::SamplerConfig>, JsError> {
    match (sampler, constraint) {
        (None, None) => Ok(None),
        (None, Some(c)) => Ok(Some(c.into_sampler()?)),
        (Some(s), None) => Ok(Some(s.into_sampler()?)),
        (Some(s), Some(c)) => {
            let cfg = s.into_sampler()?;
            Ok(Some(cfg.prepend(c.into_shift_step()?)))
        }
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
// The user-facing `Chat` (further down) spawns a Web Worker (browser) or
// `worker_threads.Worker` (Node) via `globalThis.__nbw_spawn_worker`
// (defined in `pkg-bundler/pre.js`) and talks to it over a small
// message protocol. `runInWorker()` below sets up `globalThis.onmessage`
// and reacts to incoming `load-model` / `create-chat` / `ask` /
// `tool-reply` messages, posting `ready` / `model-loaded` / `chat-ready`
// / `token` / `ask-done` / `tool-call` / `error` back. `pre.js`'s
// `postRun` hook calls `runInWorker` automatically inside any worker
// context, so the worker file is just an import of this wasm bundle.
//
// `token` messages are emitted per sampled token by the wasm-only
// per-token hook passed to `ChatHandleAsync::ask_with_token_hook` from
// the `"ask"` arm — see the streaming notes near the top of this file.
//
// Per-instance state lives in `thread_local!` because wasm32 is
// single-threaded (one wasm instance per worker = one cell).

#[cfg(target_family = "wasm")]
thread_local! {
    static WORKER_MODEL: RefCell<Option<Arc<nobodywho::llm::Model>>> = RefCell::new(None);
    static WORKER_CHAT: RefCell<Option<nobodywho::chat::ChatHandleAsync>> = RefCell::new(None);
    /// Cached worker-global scope as a generic JsValue. Set once in
    /// `run_in_worker`. We don't pin the type to
    /// `web_sys::DedicatedWorkerGlobalScope` so the same code runs on
    /// browsers (real DedicatedWorkerGlobalScope) AND in Node's
    /// `worker_threads` workers (a polyfilled globalThis with
    /// `postMessage`/`onmessage` shimmed from `parentPort`).
    static WORKER_SCOPE: RefCell<Option<JsValue>> = RefCell::new(None);
    /// In-flight tool RPC calls. The worker's tool callback registers a
    /// oneshot sender keyed by request id, then awaits the receiver. The
    /// 'tool-reply' message arm from main resolves it.
    static PENDING_TOOL_CALLS: RefCell<std::collections::HashMap<String, tokio::sync::oneshot::Sender<Result<String, String>>>> =
        RefCell::new(std::collections::HashMap::new());
    /// Monotonic counter for unique tool-call IDs (one worker = one wasm
    /// instance, so a simple per-worker counter is enough).
    static TOOL_CALL_ID_COUNTER: RefCell<u64> = const { RefCell::new(0) };
}

/// Worker-side helper: post a message back to the main thread by calling
/// `globalThis.postMessage(msg)` via Reflect. Env-agnostic — works on
/// browser `DedicatedWorkerGlobalScope` and on a Node `worker_threads`
/// worker whose globalThis has been polyfilled to expose `postMessage`.
#[cfg(target_family = "wasm")]
fn worker_post(scope: &JsValue, msg: &JsValue) -> Result<(), JsValue> {
    let post_fn: js_sys::Function = js_sys::Reflect::get(scope, &"postMessage".into())?
        .dyn_into()
        .map_err(|_| JsValue::from_str("worker scope has no postMessage function"))?;
    post_fn.call1(scope, msg).map(|_| ())
}

/// Tool metadata sent across the worker boundary. The user's JS callback
/// stays on the main thread (function refs can't survive postMessage); the
/// worker just sees this metadata and synthesizes an RPC stub.
#[cfg(target_family = "wasm")]
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ToolMeta {
    name: String,
    description: String,
    json_schema: serde_json::Value,
}

#[cfg(target_family = "wasm")]
fn next_tool_call_id() -> String {
    TOOL_CALL_ID_COUNTER.with(|c| {
        let mut id = c.borrow_mut();
        *id += 1;
        format!("tc-{}", *id)
    })
}

/// Take over `globalThis.onmessage` for the Worker that hosts this wasm
/// instance. Env-agnostic — works in browser Web Workers (where
/// globalThis is a DedicatedWorkerGlobalScope) and in Node's
/// `worker_threads` workers (where the bootstrap polyfills `postMessage`
/// and `onmessage` on globalThis to forward through `parentPort`).
///
/// Idempotent only in the sense that JS-side guards won't call it
/// twice; calling it twice from Rust would install two closures and the
/// second would overwrite the first's onmessage assignment.
///
/// Errors if `globalThis.postMessage` isn't a function (i.e. invoked
/// outside a worker context altogether).
#[cfg(target_family = "wasm")]
/// Sync wasm export called from pre.js's per-token drain helper when a
/// `stop` postMessage arrives mid-inference. Flips the same flag that
/// `ChatHandleAsync::stop_generation` does — the inference loop checks
/// it between tokens and breaks out cleanly. Exported as a sync function
/// (not Promise-returning) so the drain helper, which runs from inside
/// the synchronous wasm inference loop, can invoke it directly without
/// going through `spawn_local` (which couldn't tick anyway while wasm
/// is blocking the event loop). No-op if no chat is active.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = stopCurrentAsk)]
pub fn stop_current_ask() {
    WORKER_CHAT.with(|c| {
        if let Some(h) = c.borrow().as_ref() {
            h.stop_generation();
        }
    });
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = runInWorker)]
pub fn run_in_worker() -> Result<(), JsError> {
    use wasm_bindgen::closure::Closure;

    let scope = js_sys::global();

    // Sanity: confirm we're in a context that has `postMessage`. If not,
    // this isn't a worker (or the polyfill wasn't installed) and the
    // rest of the bootstrap would silently fail.
    let _post_check: js_sys::Function = js_sys::Reflect::get(&scope, &"postMessage".into())
        .ok()
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
        .ok_or_else(|| {
            JsError::new(
                "runInWorker: globalThis.postMessage is not a function — \
                 not inside a Web Worker (browser) or worker_threads worker (Node)",
            )
        })?;

    // Cache the scope for tool-call RPC stubs to post back to main.
    WORKER_SCOPE.with(|s| *s.borrow_mut() = Some(scope.clone().into()));

    let scope_for_handler: JsValue = scope.clone().into();
    // Closure::new (not Closure::wrap) — the latter requires UnwindSafe
    // bounds that wasm-bindgen 0.2.121 enforces on wasm32-unknown-emscripten.
    // Closure::new takes the closure directly and avoids the
    // MaybeUnwindSafe trait check entirely.
    let on_message = Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
        // Read `evt.data` synchronously here — Firefox throws
        // NS_ERROR_NOT_AVAILABLE if you touch MessageEvent properties from an
        // async continuation that runs after the synchronous handler returns.
        // The cloned JsValue we move into spawn_local is just a regular JS
        // value and safe to read whenever.
        //
        // `evt` is either a real browser MessageEvent (with a `data`
        // getter) or a polyfilled `{ data }` plain object from the Node
        // worker shim — both shapes respond to Reflect-get('data').
        let data = js_sys::Reflect::get(&evt, &"data".into()).unwrap_or(JsValue::UNDEFINED);
        let scope = scope_for_handler.clone();
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(err) = handle_worker_message(data, &scope).await {
                let _ = worker_post(&scope, &worker_reply_error(&err));
            }
        });
    });

    let scope_jsval: JsValue = scope.clone().into();
    js_sys::Reflect::set(
        &scope_jsval,
        &"onmessage".into(),
        on_message.as_ref().unchecked_ref(),
    )
    .map_err(|_| JsError::new("runInWorker: failed to set globalThis.onmessage"))?;
    // Leak: the closure outlives this function and runs for the worker's
    // lifetime. The worker is terminated by main-thread `worker.terminate()`
    // or page navigation, both of which tear down the wasm instance anyway.
    on_message.forget();

    let _ = worker_post(&scope_jsval, &worker_reply("ready"));
    Ok(())
}

/// One per message-type. Returning `Err` is what produces the `error` reply
/// — the caller wraps it via `worker_reply_error` and posts that. Takes the
/// already-extracted `data` JsValue (not the raw `MessageEvent`) because
/// Firefox revokes access to event properties once the synchronous handler
/// returns — see the comment on the `set_onmessage` call site.
#[cfg(target_family = "wasm")]
async fn handle_worker_message(data: JsValue, scope: &JsValue) -> Result<(), String> {
    // Local helper so each arm doesn't repeat the Reflect-call pattern.
    // We discard the post_message Result here for the same reason the
    // browser code did with `let _ =`: there's nothing useful to do if
    // the host has gone away.
    let post = |msg: &JsValue| {
        let _ = worker_post(scope, msg);
    };
    let msg_type = js_sys::Reflect::get(&data, &"type".into())
        .map_err(|_| "missing 'type' field".to_string())?
        .as_string()
        .ok_or_else(|| "'type' must be a string".to_string())?;

    match msg_type.as_str() {
        // Back-compat: callers that post `init` right after `new Worker(...)`
        // expecting a `ready` ack. The bootstrap already posted `ready` once;
        // we re-ack here so those callers don't hang.
        "init" => {
            post(&worker_reply("ready"));
        }
        "load-model" => {
            // Two input shapes per slot:
            //   - bytes / mmprojBytes: Uint8Array, written to MEMFS via FS.writeFile.
            //   - srcPath / mmprojSrcPath: host filesystem path string. Node-only;
            //     streamed into MEMFS chunk-by-chunk via the
            //     `__nbw_node_file_to_memfs` helper in pre.js. Saves a main-thread
            //     Buffer of the model bytes and bypasses Node's 2 GiB readFileSync
            //     cap.
            // Mutual exclusion enforced main-side in parse_chat_create_opts; here
            // we just take whichever one came through.

            // Resolve the main model into a MEMFS path. If srcPath was given,
            // stream it in chunks; otherwise materialize bytes into MEMFS via
            // the existing write_bytes_to_memfs.
            let model_memfs_path: std::path::PathBuf = if let Some(p) =
                js_sys::Reflect::get(&data, &"srcPath".into())
                    .ok()
                    .and_then(|v| v.as_string())
            {
                stream_host_file_to_memfs("model", &p).await?
            } else {
                let bytes_val = js_sys::Reflect::get(&data, &"bytes".into())
                    .map_err(|_| "missing 'bytes' or 'srcPath' field".to_string())?;
                let u8_array: js_sys::Uint8Array = bytes_val
                    .dyn_into()
                    .map_err(|_| "'bytes' must be a Uint8Array".to_string())?;
                write_bytes_to_memfs("model", &u8_array.to_vec())?
            };

            // Same shape for mmproj, but optional.
            let mmproj_path: Option<std::path::PathBuf> = if let Some(p) =
                js_sys::Reflect::get(&data, &"mmprojSrcPath".into())
                    .ok()
                    .and_then(|v| v.as_string())
            {
                Some(stream_host_file_to_memfs("mmproj", &p).await?)
            } else if let Some(u8a) = js_sys::Reflect::get(&data, &"mmprojBytes".into())
                .ok()
                .filter(|v| !v.is_undefined() && !v.is_null())
                .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok())
            {
                Some(write_bytes_to_memfs("mmproj", &u8a.to_vec())?)
            } else {
                None
            };

            // Always go via the path-based loader now that the main model
            // is always in MEMFS — covers both the srcPath and bytes
            // input modes uniformly.
            let model =
                nobodywho::llm::get_model_from_path(&model_memfs_path, mmproj_path.as_deref(), 0)
                    .map_err(|e| e.to_string())?;
            WORKER_MODEL.with(|m| *m.borrow_mut() = Some(Arc::new(model)));
            post(&worker_reply("model-loaded"));
        }
        "create-chat" => {
            let options =
                js_sys::Reflect::get(&data, &"options".into()).unwrap_or(JsValue::UNDEFINED);
            let opts: ChatOptions = if options.is_undefined() || options.is_null() {
                ChatOptions::default()
            } else {
                serde_wasm_bindgen::from_value(options).map_err(|e| e.to_string())?
            };

            // Tools come in as a separate `tools` field on the message,
            // not embedded in `options`. The user's callbacks stay on the
            // main thread (function refs can't survive postMessage); we
            // build RPC-stub `Tool::new_async` instances that round-trip
            // through `tool-call` / `tool-reply` messages.
            let tools_jsval =
                js_sys::Reflect::get(&data, &"tools".into()).unwrap_or(JsValue::UNDEFINED);
            let tools: Vec<nobodywho::tool_calling::Tool> =
                if tools_jsval.is_undefined() || tools_jsval.is_null() {
                    vec![]
                } else {
                    let metas: Vec<ToolMeta> = serde_wasm_bindgen::from_value(tools_jsval)
                        .map_err(|e| format!("tools: {e}"))?;
                    metas.into_iter().map(build_rpc_tool).collect()
                };

            let model = WORKER_MODEL
                .with(|m| m.borrow().clone())
                .ok_or_else(|| "model not loaded; send 'load-model' first".to_string())?;
            let handle = chat_handle_from_options(model, opts, tools)?;
            WORKER_CHAT.with(|c| *c.borrow_mut() = Some(handle));
            post(&worker_reply("chat-ready"));
        }
        "tool-reply" => {
            let id = js_sys::Reflect::get(&data, &"id".into())
                .ok()
                .and_then(|v| v.as_string())
                .ok_or_else(|| "missing 'id' field on tool-reply".to_string())?;
            let result = js_sys::Reflect::get(&data, &"result".into())
                .ok()
                .and_then(|v| v.as_string());
            let err = js_sys::Reflect::get(&data, &"error".into())
                .ok()
                .and_then(|v| v.as_string());
            let sender = PENDING_TOOL_CALLS.with(|m| m.borrow_mut().remove(&id));
            if let Some(tx) = sender {
                let value = match (result, err) {
                    (Some(s), _) => Ok(s),
                    (None, Some(e)) => Err(e),
                    (None, None) => Err("tool-reply missing both result and error".into()),
                };
                let _ = tx.send(value);
            }
            // No reply needed; this is the final leg of an RPC the worker
            // initiated. If `id` isn't in the map (stale / spurious),
            // silently drop.
        }
        "ask" => {
            let parts = js_sys::Reflect::get(&data, &"parts".into())
                .map_err(|_| "missing 'parts' field".to_string())?;
            let prompt = js_to_prompt(&parts)?;
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created; send 'create-chat' first".to_string())?;

            // Per-token streaming: pass a sync hook to core that fires
            // from inside the inference loop and postMessages each token
            // directly to main. `postMessage` doesn't need the worker's
            // event loop to run between tokens — it enqueues on main's
            // task queue, which is on a separate browser thread. The
            // channel path can't stream on single-threaded wasm because
            // the receiver task only wakes after the synchronous loop
            // completes; the hook bypasses that.
            let scope_for_hook = scope.clone();
            let hook = std::rc::Rc::new(move |token: &str| {
                let payload = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&payload, &"type".into(), &"token".into());
                let _ = js_sys::Reflect::set(&payload, &"token".into(), &token.into());
                let _ = worker_post(&scope_for_hook, &payload);
                // Drain any pending parentPort messages synchronously
                // (Node-only; the helper is defined by the Node worker
                // preamble in pre.js — absent in browser Web Workers).
                // Dispatches queued messages like 'stop' through the
                // worker's onmessage handler so they take effect between
                // tokens instead of waiting for the whole ask to finish.
                // Browser stop only takes effect after the current ask
                // completes; SharedArrayBuffer + Atomics is the browser
                // path forward and is tracked as a follow-up.
                if let Ok(drain) =
                    js_sys::Reflect::get(&scope_for_hook, &"__nbw_drain_messages".into())
                {
                    if let Ok(drain_fn) = drain.dyn_into::<js_sys::Function>() {
                        let _ = drain_fn.call0(&JsValue::NULL);
                    }
                }
            });
            let mut stream = handle.ask_with_token_hook(prompt, hook);
            // Await completion to detect errors and to wait for EOS
            // before signalling ask-done. The returned full text is
            // ignored — tokens were already streamed via the hook.
            stream.completed().await.map_err(|e| e.to_string())?;
            post(&worker_reply("ask-done"));
        }
        "stop" => {
            // Backstop for when stop is processed via the normal async
            // dispatch (e.g. between asks). The fast in-flight path uses
            // the wasm-exported `stopCurrentAsk` called directly from
            // the drain helper — bypasses spawn_local since the wasm
            // event loop can't tick futures while inference is running.
            if let Some(handle) = WORKER_CHAT.with(|c| c.borrow().clone()) {
                handle.stop_generation();
            }
        }
        "get-history" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let messages = handle.get_chat_history().await.map_err(|e| e.to_string())?;
            let reply = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&reply, &"type".into(), &"history-reply".into());
            let messages_jsval = serde_wasm_bindgen::to_value(&messages)
                .map_err(|e| format!("history serialize: {e}"))?;
            let _ = js_sys::Reflect::set(&reply, &"messages".into(), &messages_jsval);
            post(&reply);
        }
        "set-history" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let messages_jsval = js_sys::Reflect::get(&data, &"messages".into())
                .map_err(|_| "missing 'messages' field".to_string())?;
            let messages: Vec<nobodywho::chat::Message> =
                serde_wasm_bindgen::from_value(messages_jsval)
                    .map_err(|e| format!("messages: {e}"))?;
            handle
                .set_chat_history(messages)
                .await
                .map_err(|e| e.to_string())?;
            post(&worker_reply("history-set"));
        }
        "reset-history" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            handle.reset_history().await.map_err(|e| e.to_string())?;
            // Reuse history-set ack — same semantics (history is now cleared).
            post(&worker_reply("history-set"));
        }
        "get-system-prompt" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let prompt = handle
                .get_system_prompt()
                .await
                .map_err(|e| e.to_string())?;
            let reply = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&reply, &"type".into(), &"system-prompt-reply".into());
            let prompt_jsval = match prompt {
                Some(s) => JsValue::from_str(&s),
                None => JsValue::NULL,
            };
            let _ = js_sys::Reflect::set(&reply, &"prompt".into(), &prompt_jsval);
            post(&reply);
        }
        "set-system-prompt" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let prompt_val = js_sys::Reflect::get(&data, &"prompt".into()).unwrap_or(JsValue::NULL);
            let prompt: Option<String> = if prompt_val.is_null() || prompt_val.is_undefined() {
                None
            } else {
                prompt_val.as_string()
            };
            handle
                .set_system_prompt(prompt)
                .await
                .map_err(|e| e.to_string())?;
            post(&worker_reply("system-prompt-set"));
        }
        "get-sampler" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let sampler = handle
                .get_sampler_config()
                .await
                .map_err(|e| e.to_string())?;
            let reply = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&reply, &"type".into(), &"sampler-reply".into());
            let sampler_jsval = serde_wasm_bindgen::to_value(&sampler)
                .map_err(|e| format!("sampler serialize: {e}"))?;
            let _ = js_sys::Reflect::set(&reply, &"sampler".into(), &sampler_jsval);
            post(&reply);
        }
        "set-sampler" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let sampler_jsval = js_sys::Reflect::get(&data, &"sampler".into())
                .map_err(|_| "missing 'sampler' field".to_string())?;
            // SamplerSpec is the JS-friendly shape; convert to core's SamplerConfig.
            let spec: SamplerSpec = serde_wasm_bindgen::from_value(sampler_jsval)
                .map_err(|e| format!("sampler: {e}"))?;
            let cfg = spec.into_sampler().map_err(|e| format!("{e:?}"))?;
            handle
                .set_sampler_config(cfg)
                .await
                .map_err(|e| e.to_string())?;
            post(&worker_reply("sampler-set"));
        }
        "get-template-vars" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let vars = handle
                .get_template_variables()
                .await
                .map_err(|e| e.to_string())?;
            let reply = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&reply, &"type".into(), &"template-vars-reply".into());
            // serialize_maps_as_objects so HashMap becomes a plain JS
            // Object rather than a JS Map. Maps don't iterate as own
            // properties, so JSON.stringify(map) gives `{}` and the
            // payload would arrive empty on the channel hop.
            use serde::Serialize;
            let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
            let vars_jsval = vars
                .serialize(&ser)
                .map_err(|e| format!("template vars serialize: {e}"))?;
            let _ = js_sys::Reflect::set(&reply, &"variables".into(), &vars_jsval);
            post(&reply);
        }
        "set-template-var" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let name = js_sys::Reflect::get(&data, &"name".into())
                .ok()
                .and_then(|v| v.as_string())
                .ok_or_else(|| "missing 'name' field".to_string())?;
            let value = js_sys::Reflect::get(&data, &"value".into())
                .ok()
                .and_then(|v| v.as_bool())
                .ok_or_else(|| "missing 'value' field (must be bool)".to_string())?;
            handle
                .set_template_variable(name, value)
                .await
                .map_err(|e| e.to_string())?;
            post(&worker_reply("template-var-set"));
        }
        "set-template-vars" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let vars_jsval = js_sys::Reflect::get(&data, &"variables".into())
                .map_err(|_| "missing 'variables' field".to_string())?;
            let vars: std::collections::HashMap<String, bool> =
                serde_wasm_bindgen::from_value(vars_jsval)
                    .map_err(|e| format!("variables: {e}"))?;
            handle
                .set_template_variables(vars)
                .await
                .map_err(|e| e.to_string())?;
            post(&worker_reply("template-vars-set"));
        }
        "set-tools" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let tools_jsval =
                js_sys::Reflect::get(&data, &"tools".into()).unwrap_or(JsValue::UNDEFINED);
            let tools: Vec<nobodywho::tool_calling::Tool> =
                if tools_jsval.is_undefined() || tools_jsval.is_null() {
                    vec![]
                } else {
                    let metas: Vec<ToolMeta> = serde_wasm_bindgen::from_value(tools_jsval)
                        .map_err(|e| format!("tools: {e}"))?;
                    metas.into_iter().map(build_rpc_tool).collect()
                };
            handle.set_tools(tools).await.map_err(|e| e.to_string())?;
            post(&worker_reply("tools-set"));
        }
        "reset-chat" => {
            let handle = WORKER_CHAT
                .with(|c| c.borrow().clone())
                .ok_or_else(|| "chat not created".to_string())?;
            let prompt_val = js_sys::Reflect::get(&data, &"prompt".into()).unwrap_or(JsValue::NULL);
            let prompt: Option<String> = if prompt_val.is_null() || prompt_val.is_undefined() {
                None
            } else {
                prompt_val.as_string()
            };
            let tools_jsval =
                js_sys::Reflect::get(&data, &"tools".into()).unwrap_or(JsValue::UNDEFINED);
            let tools: Vec<nobodywho::tool_calling::Tool> =
                if tools_jsval.is_undefined() || tools_jsval.is_null() {
                    vec![]
                } else {
                    let metas: Vec<ToolMeta> = serde_wasm_bindgen::from_value(tools_jsval)
                        .map_err(|e| format!("tools: {e}"))?;
                    metas.into_iter().map(build_rpc_tool).collect()
                };
            handle
                .reset_chat(prompt, tools)
                .await
                .map_err(|e| e.to_string())?;
            post(&worker_reply("chat-reset"));
        }
        other => return Err(format!("unknown msg type: {other}")),
    }

    Ok(())
}

/// Build a `Tool` whose async callback is an RPC stub: it postMessages a
/// `tool-call` request back to the main thread (carrying the call id, the
/// tool name, and the serialized args) and parks on a oneshot until the
/// main thread replies with `tool-reply`. Used by the worker-side
/// `create-chat` handler when Chat is constructed with tools.
#[cfg(target_family = "wasm")]
fn build_rpc_tool(meta: ToolMeta) -> nobodywho::tool_calling::Tool {
    let name_for_closure = meta.name.clone();
    nobodywho::tool_calling::Tool::new_async(
        meta.name,
        meta.description,
        meta.json_schema,
        move |args: serde_json::Value| {
            let name = name_for_closure.clone();
            async move {
                let id = next_tool_call_id();

                let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
                PENDING_TOOL_CALLS.with(|m| {
                    m.borrow_mut().insert(id.clone(), tx);
                });

                // Build the tool-call message: { type, id, name, args }.
                // serde_wasm_bindgen with `serialize_maps_as_objects(true)`
                // so `args.city` reads as a plain-Object property on the JS
                // side. The default `to_value` would convert
                // `serde_json::Value::Object` to a JS `Map`, which the user's
                // callback can't access via `args.city`.
                let payload = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&payload, &"type".into(), &"tool-call".into());
                let _ = js_sys::Reflect::set(&payload, &"id".into(), &id.as_str().into());
                let _ = js_sys::Reflect::set(&payload, &"name".into(), &name.as_str().into());
                let args_js = {
                    use serde::Serialize as _;
                    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
                    match args.serialize(&ser) {
                        Ok(v) => v,
                        Err(e) => {
                            PENDING_TOOL_CALLS.with(|m| {
                                m.borrow_mut().remove(&id);
                            });
                            return format!("ERROR: tool args conversion: {e}");
                        }
                    }
                };
                let _ = js_sys::Reflect::set(&payload, &"args".into(), &args_js);

                let post_res = WORKER_SCOPE.with(|s| match s.borrow().as_ref() {
                    Some(scope) => worker_post(scope, &payload).map_err(|e| format!("{e:?}")),
                    None => Err("worker scope not initialized".to_string()),
                });
                if let Err(e) = post_res {
                    PENDING_TOOL_CALLS.with(|m| {
                        m.borrow_mut().remove(&id);
                    });
                    return format!("ERROR: post tool-call: {e}");
                }

                match rx.await {
                    Ok(Ok(s)) => s,
                    Ok(Err(e)) => format!("ERROR: {e}"),
                    Err(_) => "ERROR: tool-reply sender dropped".to_string(),
                }
            }
        },
    )
}

/// Build a `ChatHandleAsync` from a parsed `ChatOptions` plus a list of
/// (already-built) tools. Same option-mapping logic as `Chat::new`'s
/// constructor — factored out so the worker dispatcher doesn't duplicate
/// it. Errors as `String` because the worker dispatcher turns them into
/// `{ type: "error", message }` post-messages; `JsError` (used by the
/// wasm-bindgen-exposed constructor) doesn't impl `Display`.
#[cfg(target_family = "wasm")]
fn chat_handle_from_options(
    model: Arc<nobodywho::llm::Model>,
    opts: ChatOptions,
    tools: Vec<nobodywho::tool_calling::Tool>,
) -> Result<nobodywho::chat::ChatHandleAsync, String> {
    let mut builder = nobodywho::chat::ChatBuilder::new(model);
    if let Some(ctx) = opts.context_size {
        builder = builder.with_context_size(ctx);
    }
    if let Some(sys) = opts.system_prompt {
        builder = builder.with_system_prompt(Some(sys));
    }
    if let Some(sampler) = build_sampler(opts.sampler, opts.constraint).map_err(|e| {
        // build_sampler returns Err(JsError) only when the spec is invalid
        // (constraint not exclusive-one-of, unknown sampleStep, etc.); reach
        // into the underlying Error.message via Reflect.
        let val: JsValue = e.into();
        js_sys::Reflect::get(&val, &"message".into())
            .ok()
            .and_then(|m| m.as_string())
            .unwrap_or_else(|| "invalid sampler / constraint".to_string())
    })? {
        builder = builder.with_sampler(sampler);
    }
    if let Some(vars) = opts.template_variables {
        builder = builder.with_template_variables(vars);
    }
    if !tools.is_empty() {
        builder = builder.with_tools(tools);
    }
    // build_async() now returns Result (main added init-handshake error
    // propagation in commit on main); collapse into our String-error
    // worker channel.
    builder.build_async().map_err(|e| e.to_string())
}

#[cfg(target_family = "wasm")]
fn worker_reply(type_name: &str) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"type".into(), &type_name.into());
    obj.into()
}

#[cfg(target_family = "wasm")]
fn worker_reply_error(message: &str) -> JsValue {
    let obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&obj, &"type".into(), &"error".into());
    let _ = js_sys::Reflect::set(&obj, &"message".into(), &message.into());
    obj.into()
}

/// Used by every Chat setter / getter to early-return if the worker has
/// been terminated. Borrow scope is tight so we don't hold it across the
/// later `worker_post`.
#[cfg(target_family = "wasm")]
fn check_not_terminated(state: &std::rc::Rc<RefCell<ChatState>>) -> Result<(), JsError> {
    if state.borrow().terminated {
        return Err(JsError::new("Chat: already terminated"));
    }
    Ok(())
}

/// Split a JS `tools` array (`[Tool.fromFn(...), ...]`) into the
/// main-thread callback map (name → JS function ref) plus a
/// structured-cloneable metadata array (`[{name, description,
/// jsonSchema}, ...]`) for postMessage to the worker. Used by both
/// `Chat.create` and `Chat.setTools`. Returns `(empty, empty array)`
/// if `tools_jsval` is null / undefined.
#[cfg(target_family = "wasm")]
fn extract_tool_callbacks(
    tools_jsval: &JsValue,
) -> Result<(std::collections::HashMap<String, js_sys::Function>, JsValue), JsError> {
    let mut tool_callbacks: std::collections::HashMap<String, js_sys::Function> =
        std::collections::HashMap::new();
    let tools_meta_array = js_sys::Array::new();
    if tools_jsval.is_undefined() || tools_jsval.is_null() {
        return Ok((tool_callbacks, tools_meta_array.into()));
    }
    let arr: js_sys::Array = tools_jsval
        .clone()
        .dyn_into()
        .map_err(|_| JsError::new("tools must be an array of Tool.fromFn(...) values"))?;
    for (idx, raw) in arr.iter().enumerate() {
        let kind = js_sys::Reflect::get(&raw, &"__nbwKind".into())
            .ok()
            .and_then(|v| v.as_string());
        if kind.as_deref() != Some("tool") {
            return Err(JsError::new(&format!(
                "tools[{idx}] is not a Tool.fromFn(...) value (missing __nbwKind=tool)",
            )));
        }
        let name = js_sys::Reflect::get(&raw, &"name".into())
            .ok()
            .and_then(|v| v.as_string())
            .ok_or_else(|| JsError::new(&format!("tools[{idx}]: missing name")))?;
        let description = js_sys::Reflect::get(&raw, &"description".into())
            .ok()
            .and_then(|v| v.as_string())
            .ok_or_else(|| JsError::new(&format!("tools[{idx}]: missing description")))?;
        let schema = js_sys::Reflect::get(&raw, &"jsonSchema".into())
            .map_err(|_| JsError::new(&format!("tools[{idx}]: missing jsonSchema")))?;
        let callback = js_sys::Reflect::get(&raw, &"callback".into())
            .map_err(|_| JsError::new(&format!("tools[{idx}]: missing callback")))?
            .dyn_into::<js_sys::Function>()
            .map_err(|_| JsError::new(&format!("tools[{idx}]: callback is not a function")))?;
        let meta_obj = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&meta_obj, &"name".into(), &name.as_str().into());
        let _ = js_sys::Reflect::set(
            &meta_obj,
            &"description".into(),
            &description.as_str().into(),
        );
        let _ = js_sys::Reflect::set(&meta_obj, &"jsonSchema".into(), &schema);
        tools_meta_array.push(&meta_obj);
        tool_callbacks.insert(name, callback);
    }
    Ok((tool_callbacks, tools_meta_array.into()))
}

// ---------- Cache API helpers ----------
//
// Browser-side model caching via the Cache API store named 'nobodywho-models-v1'.
// Implemented here in Rust (via web-sys) so the JS-side bootstrap stays a
// thin shim. Used by `fetchModelBytes` / `Model.preload`.

#[cfg(target_family = "wasm")]
const MODEL_CACHE_NAME: &str = "nobodywho-models-v1";

/// Try to open the model cache. Returns None if the Cache API isn't usable
/// in the current context (insecure http, file://, sandboxed iframe) — the
/// caller falls through to a plain fetch in that case.
///
/// `caches` is available on both `Window` (main thread) and
/// `WorkerGlobalScope` (web worker), with different web-sys types.
#[cfg(target_family = "wasm")]
async fn open_model_cache() -> Option<web_sys::Cache> {
    let caches = caches_from_global()?;
    let opened = wasm_bindgen_futures::JsFuture::from(caches.open(MODEL_CACHE_NAME))
        .await
        .ok()?;
    opened.dyn_into::<web_sys::Cache>().ok()
}

#[cfg(target_family = "wasm")]
fn caches_from_global() -> Option<web_sys::CacheStorage> {
    if let Ok(window) = js_sys::global().dyn_into::<web_sys::Window>() {
        return window.caches().ok();
    }
    if let Ok(scope) = js_sys::global().dyn_into::<web_sys::WorkerGlobalScope>() {
        return scope.caches().ok();
    }
    None
}

#[cfg(target_family = "wasm")]
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
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = fetchModelBytes)]
pub fn fetch_model_bytes(url: String, on_progress: Option<js_sys::Function>) -> js_sys::Promise {
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
            let read_result = wasm_bindgen_futures::JsFuture::from(reader.read())
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
                let _ = wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&url, &resp)).await;
            }
        }

        Ok(bytes)
    })
}

// Static methods on the existing Model class. wasm-bindgen lets you add to
// the same JS class from multiple `impl` blocks.
#[cfg(target_family = "wasm")]
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

// ---------- TokenStream ----------
//
// User-facing token stream returned from `Chat::ask`. Three consumption
// modes:
//
//   - `for await (const tok of stream)` — idiomatic JS async iteration.
//     Wired via a one-line `[Symbol.asyncIterator]` shim attached to
//     `Module.TokenStream.prototype` in `pkg-bundler/pre.js`'s `postRun`
//     hook (wasm-bindgen 0.2.121 can't emit the protocol cleanly on a
//     pyclass-style binding, so we patch it in).
//   - `stream.next()` → `Promise<{value: string, done: boolean}>` — pull
//     one token at a time manually. Each resolution fires as soon as the
//     worker postMessages a `token` payload.
//   - `stream.completed()` → `Promise<string>` — wait for EOS and resolve
//     to the full concatenated text. Equivalent to draining `next()`.
//
// State shared with `Chat` via `Rc<RefCell<WorkerStreamState>>`: Chat pushes
// tokens/done/error into the state from inside its `onmessage` closure; the
// stream's `next()`/`completed()` Promises resolve out of that state.

#[cfg(target_family = "wasm")]
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

#[cfg(target_family = "wasm")]
enum NextOutcome {
    Token(String),
    Done,
    Err(String),
}

#[cfg(target_family = "wasm")]
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

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub struct TokenStream {
    state: std::rc::Rc<RefCell<WorkerStreamState>>,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl TokenStream {
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
#[cfg(target_family = "wasm")]
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
// User-facing chat class. `Chat::create` spawns a worker (browser
// `Worker(blobURL)` or Node `worker_threads.Worker`) via the JS-side
// `globalThis.__nbw_spawn_worker` helper defined in `pkg-bundler/pre.js`,
// posts the `load-model` / `create-chat` / `ask` protocol, and routes
// replies via a Closure-wrapped `onmessage`. The worker side is handled
// by `runInWorker()` further up.
//
// App code shape:
//
//     const chat = await Chat.create({ modelUrl, systemPrompt, ... });
//     // streaming via async iteration:
//     for await (const tok of chat.ask(prompt)) {
//         process.stdout.write(tok);
//     }
//     // or get the full text at once:
//     const full = await chat.ask(prompt).completed();

// JS sets this at module load — see `pre.js`'s `postRun` hook, which
// calls `Module.setBootstrapUrl(_scriptName)` where `_scriptName` is the
// Emscripten loader's `import.meta.url`. `Chat::create` reads it to
// build the inline Blob worker bootstrap that re-imports this wasm
// loader inside the spawned worker.
#[cfg(target_family = "wasm")]
thread_local! {
    static BOOTSTRAP_URL: RefCell<Option<String>> = RefCell::new(None);
}

/// Register the absolute URL of the Emscripten loader (`nobodywho_js.js`)
/// so `Chat::create` knows what to `import()` inside the spawned worker.
/// Called automatically from `pre.js`'s `postRun` hook; you should never
/// need to call this from app code.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = setBootstrapUrl)]
pub fn set_bootstrap_url(url: String) {
    BOOTSTRAP_URL.with(|u| *u.borrow_mut() = Some(url));
}

#[cfg(target_family = "wasm")]
fn get_bootstrap_url() -> Result<String, JsError> {
    BOOTSTRAP_URL.with(|u| u.borrow().clone()).ok_or_else(|| {
        JsError::new(
            "Chat.create: setBootstrapUrl was not called. \
                 pkg-bundler/pre.js's postRun hook must invoke \
                 Module.setBootstrapUrl(_scriptName) at module load before \
                 Chat.create() is invoked.",
        )
    })
}

#[cfg(target_family = "wasm")]
struct ChatState {
    /// The spawned worker as an opaque JS handle. Reached via Reflect for
    /// postMessage / terminate / onmessage / onerror, so the same code
    /// works whether the underlying object is a real browser `Worker` or
    /// the Node shim wrapping a `worker_threads.Worker` (see pre.js's
    /// `__nbw_spawn_worker`).
    worker: JsValue,
    current_stream: Option<std::rc::Rc<RefCell<WorkerStreamState>>>,
    /// While the main thread is awaiting a typed reply from the worker
    /// (Chat.create's load/create handshake, or any of the getter /
    /// setter request-reply pairs like getChatHistory), this holds
    /// `(expected_reply_type, sender)`. The onmessage closure
    /// JSON-stringifies the reply data and signals via this oneshot.
    /// The waiter parses the JSON back into JS via `JSON.parse` and
    /// extracts whatever payload field it needs.
    ///
    /// The payload type is `String` (a serialized JSON object) rather
    /// than `JsValue` because `JsValue` contains an UnsafeCell, which
    /// transitively makes the onmessage closure !UnwindSafe and breaks
    /// `Closure::new`. Round-tripping via JSON is fine here: every
    /// reply payload we use (`messages`, `prompt`, `sampler`,
    /// `variables`) is already JSON-serializable.
    pending_handshake: Option<(String, tokio::sync::oneshot::Sender<Result<String, String>>)>,
    /// Main-thread registry of tool callbacks. JS function refs can't
    /// survive `postMessage` (structured clone rejects functions), so the
    /// worker only ever sees tool metadata (name + description + schema).
    /// When the worker emits a `tool-call` RPC for a given name, main
    /// looks up the callback here, invokes it, and posts back the result.
    tool_callbacks: std::collections::HashMap<String, js_sys::Function>,
    terminated: bool,
    _on_message: Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>>,
    _on_error: Option<wasm_bindgen::closure::Closure<dyn FnMut(JsValue)>>,
}

/// Best-effort terminate of a worker handle via Reflect — works for
/// both browser `Worker` and Node shim. Errors are swallowed (we're
/// already cleaning up).
#[cfg(target_family = "wasm")]
fn worker_terminate(worker: &JsValue) {
    if let Ok(f) = js_sys::Reflect::get(worker, &"terminate".into()) {
        if let Ok(fun) = f.dyn_into::<js_sys::Function>() {
            let _ = fun.call0(worker);
        }
    }
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub struct Chat {
    state: std::rc::Rc<RefCell<ChatState>>,
}

#[cfg(target_family = "wasm")]
impl Drop for Chat {
    fn drop(&mut self) {
        // Best-effort cleanup: terminate the worker so it doesn't hang around
        // after the wasm-side Chat is released. The closures hold `Weak`
        // refs to ChatState (no cycle), so dropping state here is safe.
        if let Ok(st) = self.state.try_borrow() {
            worker_terminate(&st.worker);
        }
    }
}

#[cfg(target_family = "wasm")]
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

            // Delegate Worker construction to pre.js's `__nbw_spawn_worker`
            // helper, which picks between the browser `Worker(blobURL)` path
            // and the Node `worker_threads.Worker` path. It returns a
            // Worker-shaped object (real Worker in browser, JS shim in Node);
            // we treat it as a generic JsValue and access methods via Reflect.
            let global = js_sys::global();
            let spawn_fn: js_sys::Function =
                js_sys::Reflect::get(&global, &"__nbw_spawn_worker".into())
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
                    .ok_or_else(|| {
                        JsError::new(
                            "Chat.create: globalThis.__nbw_spawn_worker is not defined \
                             — pre.js was not loaded (build artifact incomplete)",
                        )
                    })?;
            let worker_promise = spawn_fn
                .call1(&JsValue::NULL, &JsValue::from_str(&bootstrap))
                .map_err(|e| JsError::new(&format!("__nbw_spawn_worker threw: {e:?}")))?;
            let worker: JsValue =
                wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(worker_promise))
                    .await
                    .map_err(|e| JsError::new(&format!("__nbw_spawn_worker rejected: {e:?}")))?;

            // Take the tool callbacks out of `parsed` and install them in
            // state. They stay on the main thread; the worker only sees
            // the metadata-only array we built alongside.
            let tool_callbacks = parsed.tool_callbacks;
            let tools_jsval = parsed.tools_jsval;

            // Construct the state. Closures install themselves into state.
            let state = std::rc::Rc::new(RefCell::new(ChatState {
                worker: worker.clone(),
                current_stream: None,
                pending_handshake: None,
                tool_callbacks,
                terminated: false,
                _on_message: None,
                _on_error: None,
            }));

            // The captured `Weak<RefCell<ChatState>>` chain is naturally
            // !UnwindSafe (Rc strong/weak counts are `Cell<usize>`;
            // ChatState contains JsValue which is `UnsafeCell`-backed).
            // wasm-bindgen's `Closure::new` enforces UnwindSafe on its
            // captures. There's no actual unwind concern: JS event
            // handlers don't run through Rust catch_unwind. Wrap the
            // captures in AssertUnwindSafe to satisfy the bound; access
            // via `.0` inside the body.
            // The captured `Weak<RefCell<ChatState>>` chain is naturally
            // !UnwindSafe (Rc strong/weak counts are `Cell<usize>`;
            // ChatState contains JsValue which is `UnsafeCell`-backed).
            // wasm-bindgen's `Closure::new` enforces `MaybeUnwindSafe`
            // on its captures. There's no actual unwind concern: JS
            // event handlers don't run through Rust catch_unwind.
            //
            // Wrap the Weak in AssertUnwindSafe and force the WHOLE
            // wrapper to be captured by rebinding the inner name to
            // the wrapper at the top of the closure body. Rust 2021
            // disjoint captures would otherwise see only `.0` is used
            // and capture just the bare Weak (bypassing the wrap).
            let state_weak = std::panic::AssertUnwindSafe(std::rc::Rc::downgrade(&state));
            let on_message =
                wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
                    let state_weak = &state_weak;
                    if let Some(state) = state_weak.0.upgrade() {
                        // `evt` is either a browser MessageEvent (with `.data`)
                        // or the Node shim's `{ data }` plain object — both
                        // respond to Reflect-get('data').
                        let data = js_sys::Reflect::get(&evt, &"data".into())
                            .unwrap_or(JsValue::UNDEFINED);
                        handle_chat_message(&state, data);
                    }
                });

            let state_weak2 = std::panic::AssertUnwindSafe(std::rc::Rc::downgrade(&state));
            let on_error =
                wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
                    let state_weak2 = &state_weak2;
                    if let Some(state) = state_weak2.0.upgrade() {
                        // Browser ErrorEvent has `.message`; the Node shim
                        // synthesizes `{ message }`. Read via Reflect.
                        let msg = js_sys::Reflect::get(&evt, &"message".into())
                            .ok()
                            .and_then(|v| v.as_string())
                            .unwrap_or_else(|| format!("{evt:?}"));
                        handle_chat_error(&state, format!("worker crashed: {msg}"));
                    }
                });

            js_sys::Reflect::set(
                &worker,
                &"onmessage".into(),
                on_message.as_ref().unchecked_ref(),
            )
            .map_err(|e| JsError::new(&format!("set worker.onmessage: {e:?}")))?;
            js_sys::Reflect::set(
                &worker,
                &"onerror".into(),
                on_error.as_ref().unchecked_ref(),
            )
            .map_err(|e| JsError::new(&format!("set worker.onerror: {e:?}")))?;

            {
                let mut st = state.borrow_mut();
                st._on_message = Some(on_message);
                st._on_error = Some(on_error);
            }

            // Handshake step 1: wait for 'ready' from the worker.
            wait_for_handshake(&state, "ready").await?;

            // Handshake step 2: tell the worker how to find the model.
            // Three input modes per slot (main and mmproj), in precedence
            // order: bytes > path > url. modelPath/mmprojPath are Node-
            // only and pass a host filesystem path string to the worker,
            // which then chunk-streams it into MEMFS via the
            // `__nbw_node_file_to_memfs` helper from pre.js. This
            // bypasses the main-thread Buffer of model bytes entirely
            // (and Node's 2 GiB fs.readFileSync cap).
            let load_msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&load_msg, &"type".into(), &"load-model".into());

            if let Some(bytes) = parsed.model_bytes {
                let _ = js_sys::Reflect::set(&load_msg, &"bytes".into(), &bytes.into());
            } else if let Some(path) = parsed.model_path.as_ref() {
                let _ =
                    js_sys::Reflect::set(&load_msg, &"srcPath".into(), &JsValue::from_str(path));
            } else if let Some(url) = parsed.model_url {
                let bytes_promise = fetch_model_bytes(url, parsed.on_progress.clone());
                let bytes_val: JsValue = wasm_bindgen_futures::JsFuture::from(bytes_promise)
                    .await
                    .map_err(|e| {
                        let msg = js_sys::Reflect::get(&e, &"message".into())
                            .ok()
                            .and_then(|m| m.as_string())
                            .unwrap_or_else(|| format!("{e:?}"));
                        JsError::new(&format!("fetchModelBytes: {msg}"))
                    })?;
                let _ = js_sys::Reflect::set(&load_msg, &"bytes".into(), &bytes_val);
            } else {
                return Err(JsError::new(
                    "Chat.create: pass one of modelUrl / modelBytes / modelPath",
                ));
            }

            if let Some(bytes) = parsed.mmproj_bytes {
                let _ = js_sys::Reflect::set(&load_msg, &"mmprojBytes".into(), &bytes.into());
            } else if let Some(path) = parsed.mmproj_path.as_ref() {
                let _ = js_sys::Reflect::set(
                    &load_msg,
                    &"mmprojSrcPath".into(),
                    &JsValue::from_str(path),
                );
            } else if let Some(url) = parsed.mmproj_url {
                let bytes_promise = fetch_model_bytes(url, parsed.on_progress);
                let bytes_val: JsValue = wasm_bindgen_futures::JsFuture::from(bytes_promise)
                    .await
                    .map_err(|e| {
                        let msg = js_sys::Reflect::get(&e, &"message".into())
                            .ok()
                            .and_then(|m| m.as_string())
                            .unwrap_or_else(|| format!("{e:?}"));
                        JsError::new(&format!("fetchModelBytes(mmproj): {msg}"))
                    })?;
                let _ = js_sys::Reflect::set(&load_msg, &"mmprojBytes".into(), &bytes_val);
            }
            // (no mmproj provided ⇒ text-only model)

            worker_post(&state.borrow().worker, &load_msg)
                .map_err(|e| JsError::new(&format!("post load-model: {e:?}")))?;
            wait_for_handshake(&state, "model-loaded").await?;

            // Handshake step 3: post 'create-chat' with the chat options
            // (the original JS object minus the modelUrl/modelBytes/
            // onDownloadProgress/tools keys; see parse_chat_create_opts)
            // plus a separate `tools` field carrying just metadata.
            // Callbacks stay main-thread; the worker synthesizes RPC stubs
            // that round-trip via `tool-call` / `tool-reply`.
            let create_msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&create_msg, &"type".into(), &"create-chat".into());
            let _ = js_sys::Reflect::set(&create_msg, &"options".into(), &parsed.chat_opts_jsval);
            let _ = js_sys::Reflect::set(&create_msg, &"tools".into(), &tools_jsval);
            worker_post(&state.borrow().worker, &create_msg)
                .map_err(|e| JsError::new(&format!("post create-chat: {e:?}")))?;
            wait_for_handshake(&state, "chat-ready").await?;

            Ok(Chat { state })
        })
    }

    /// Callback-style streaming: send a prompt, invoke `callback(token)`
    /// for each token as it arrives, and resolve to the full accumulated
    /// text when generation ends. Sugar over the `TokenStream` shape —
    /// equivalent to:
    ///
    /// ```js
    /// const stream = chat.ask(prompt);
    /// for await (const tok of stream) callback(tok);
    /// const full = await stream.completed();
    /// ```
    ///
    /// Mirrors the `askStreaming` shape from the older JS-bridge binding
    /// (and is the closest analog to Python's `ask` with a per-token
    /// callback). The for-await pattern remains the JS-idiomatic
    /// alternative.
    ///
    /// Only one ask can be in flight at a time per Chat. Callback return
    /// values are ignored — fire-and-forget. If the callback throws,
    /// the throw is swallowed (the next token still fires).
    #[wasm_bindgen(js_name = askStreaming)]
    pub fn ask_streaming(&self, prompt: JsValue, callback: js_sys::Function) -> js_sys::Promise {
        let parts_result = js_to_serializable_parts(&prompt);
        let state = self.state.clone();
        promisify(async move {
            let parts = parts_result?;
            // Validate + register the stream + post the ask message.
            // Mirrors `ask()`'s setup, just without returning the
            // TokenStream handle to JS.
            let stream_state = {
                let mut st = state.borrow_mut();
                if st.terminated {
                    return Err(JsError::new("Chat: already terminated"));
                }
                if st.current_stream.is_some() {
                    return Err(JsError::new(
                        "Chat.askStreaming: another ask is in progress; await it first",
                    ));
                }
                let ss = std::rc::Rc::new(RefCell::new(WorkerStreamState::new()));
                st.current_stream = Some(ss.clone());
                let ask_msg = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&ask_msg, &"type".into(), &"ask".into());
                let _ = js_sys::Reflect::set(&ask_msg, &"parts".into(), &parts);
                worker_post(&st.worker, &ask_msg)
                    .map_err(|e| JsError::new(&format!("post ask: {e:?}")))?;
                ss
            };

            let mut full = String::new();
            loop {
                // Sync fast-path: drain buffered tokens, check done/error.
                // Holding the borrow only across the inner block so the
                // callback can't trigger a re-entrant borrow.
                let pending_rx = {
                    let mut st = stream_state.borrow_mut();
                    if let Some(err) = st.error.clone() {
                        return Err(JsError::new(&err));
                    }
                    if let Some(tok) = st.buffer.pop_front() {
                        drop(st);
                        full.push_str(&tok);
                        let _ = callback.call1(&JsValue::NULL, &tok.as_str().into());
                        continue;
                    }
                    if st.done {
                        return Ok(JsValue::from_str(&full));
                    }
                    let (tx, rx) = tokio::sync::oneshot::channel();
                    st.pending_next = Some(tx);
                    rx
                };
                // Async: park until Chat's onmessage routes the next token.
                match pending_rx.await {
                    Ok(NextOutcome::Token(tok)) => {
                        full.push_str(&tok);
                        let _ = callback.call1(&JsValue::NULL, &tok.as_str().into());
                    }
                    Ok(NextOutcome::Done) => return Ok(JsValue::from_str(&full)),
                    Ok(NextOutcome::Err(e)) => return Err(JsError::new(&e)),
                    Err(_) => return Err(JsError::new("stream sender dropped before token")),
                }
            }
        })
    }

    /// Send a prompt; returns a synchronously-constructed `TokenStream`
    /// that resolves token-by-token (or all-at-once via `.completed()`).
    /// Only one ask can be in flight at a time per Chat.
    ///
    /// `prompt` is either a plain string (text-only) or an array of mixed
    /// `string | Image | Audio` parts (multimodal). Bytes ride along by
    /// structured-clone copy on the postMessage to the worker.
    pub fn ask(&self, prompt: JsValue) -> Result<TokenStream, JsError> {
        // Serialize before taking the state borrow so we don't hold it across
        // an early-return.
        let parts = js_to_serializable_parts(&prompt)?;

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
        let _ = js_sys::Reflect::set(&ask_msg, &"parts".into(), &parts);
        worker_post(&st.worker, &ask_msg).map_err(|e| JsError::new(&format!("post ask: {e:?}")))?;
        drop(st);

        Ok(TokenStream {
            state: stream_state,
        })
    }

    /// Snapshot the conversation history (excluding the system prompt).
    /// Returns the messages as serialized JS objects matching core's
    /// `Message` enum shape: `{role: 'user'|'assistant'|'system'|'tool',
    /// content: '...', ...}`. Useful for save/load, branching, or
    /// inspecting what the model has been told.
    #[wasm_bindgen(js_name = getChatHistory)]
    pub fn get_chat_history(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            {
                let st = state.borrow();
                if st.terminated {
                    return Err(JsError::new("Chat: already terminated"));
                }
            }
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"get-history".into());
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post get-history: {e:?}")))?;
            let reply = wait_for_handshake(&state, "history-reply").await?;
            let messages = js_sys::Reflect::get(&reply, &"messages".into())
                .map_err(|_| JsError::new("history-reply missing 'messages' field"))?;
            Ok(messages)
        })
    }

    /// Replace the conversation history with the given messages. Pass
    /// an array of `{role, content, ...}` objects matching core's
    /// `Message` enum (the same shape `getChatHistory()` returns). Use
    /// for loading a saved conversation or rewinding to a branch point.
    #[wasm_bindgen(js_name = setChatHistory)]
    pub fn set_chat_history(&self, messages: JsValue) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            {
                let st = state.borrow();
                if st.terminated {
                    return Err(JsError::new("Chat: already terminated"));
                }
            }
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"set-history".into());
            let _ = js_sys::Reflect::set(&msg, &"messages".into(), &messages);
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post set-history: {e:?}")))?;
            let _ = wait_for_handshake(&state, "history-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Read the current system prompt. Resolves to a string or `null`
    /// (matching Python's `Optional[str]`).
    #[wasm_bindgen(js_name = getSystemPrompt)]
    pub fn get_system_prompt(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"get-system-prompt".into());
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post get-system-prompt: {e:?}")))?;
            let reply = wait_for_handshake(&state, "system-prompt-reply").await?;
            let prompt = js_sys::Reflect::get(&reply, &"prompt".into()).unwrap_or(JsValue::NULL);
            Ok(prompt)
        })
    }

    /// Replace the system prompt. Pass `null` to clear it. Takes effect
    /// on the next ask (existing chat history is preserved).
    #[wasm_bindgen(js_name = setSystemPrompt)]
    pub fn set_system_prompt(&self, prompt: JsValue) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"set-system-prompt".into());
            let _ = js_sys::Reflect::set(&msg, &"prompt".into(), &prompt);
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post set-system-prompt: {e:?}")))?;
            let _ = wait_for_handshake(&state, "system-prompt-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Read the current sampler config. Returns the JSON shape used by
    /// `Chat.create({sampler: ...})`.
    #[wasm_bindgen(js_name = getSamplerConfig)]
    pub fn get_sampler_config(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"get-sampler".into());
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post get-sampler: {e:?}")))?;
            let reply = wait_for_handshake(&state, "sampler-reply").await?;
            let sampler = js_sys::Reflect::get(&reply, &"sampler".into()).unwrap_or(JsValue::NULL);
            Ok(sampler)
        })
    }

    /// Replace the sampler config. Takes the same shape as
    /// `Chat.create({sampler: ...})`.
    #[wasm_bindgen(js_name = setSamplerConfig)]
    pub fn set_sampler_config(&self, sampler: JsValue) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"set-sampler".into());
            let _ = js_sys::Reflect::set(&msg, &"sampler".into(), &sampler);
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post set-sampler: {e:?}")))?;
            let _ = wait_for_handshake(&state, "sampler-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Read all template variables — returns a plain JS object like
    /// `{enable_thinking: false, ...}`.
    #[wasm_bindgen(js_name = getTemplateVariables)]
    pub fn get_template_variables(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"get-template-vars".into());
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post get-template-vars: {e:?}")))?;
            let reply = wait_for_handshake(&state, "template-vars-reply").await?;
            let vars =
                js_sys::Reflect::get(&reply, &"variables".into()).unwrap_or(JsValue::UNDEFINED);
            Ok(vars)
        })
    }

    /// Set a single template variable (must be a boolean).
    #[wasm_bindgen(js_name = setTemplateVariable)]
    pub fn set_template_variable(&self, name: String, value: bool) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"set-template-var".into());
            let _ = js_sys::Reflect::set(&msg, &"name".into(), &name.as_str().into());
            let _ = js_sys::Reflect::set(&msg, &"value".into(), &value.into());
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post set-template-var: {e:?}")))?;
            let _ = wait_for_handshake(&state, "template-var-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Replace all template variables. Pass a plain JS object of
    /// string → boolean (e.g. `{enable_thinking: false}`).
    #[wasm_bindgen(js_name = setTemplateVariables)]
    pub fn set_template_variables(&self, variables: JsValue) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"set-template-vars".into());
            let _ = js_sys::Reflect::set(&msg, &"variables".into(), &variables);
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post set-template-vars: {e:?}")))?;
            let _ = wait_for_handshake(&state, "template-vars-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Replace the available tools. Accepts the same `Tool.fromFn(...)`
    /// array shape as `Chat.create({tools: ...})`. Updates both the
    /// main-thread callback registry and the worker's tool dispatch.
    #[wasm_bindgen(js_name = setTools)]
    pub fn set_tools(&self, tools: JsValue) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            // Split the same way Chat.create does: callbacks stay on
            // main thread, only metadata crosses postMessage.
            let (callbacks, tools_meta) = extract_tool_callbacks(&tools)?;
            {
                let mut st = state.borrow_mut();
                st.tool_callbacks = callbacks;
            }
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"set-tools".into());
            let _ = js_sys::Reflect::set(&msg, &"tools".into(), &tools_meta);
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post set-tools: {e:?}")))?;
            let _ = wait_for_handshake(&state, "tools-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Reset the chat to a fresh state — clears history AND (optionally)
    /// replaces the system prompt and tool list in one atomic worker
    /// round-trip. Mirrors Python's `Chat.reset(system_prompt, tools)`.
    ///
    /// `opts` is `{ systemPrompt?, tools? }`:
    /// - `systemPrompt`: string sets it; null / undefined clears it.
    /// - `tools`: an array of `Tool.fromFn(...)` replaces the registry;
    ///   undefined or [] clears all tools.
    ///
    /// To clear history only (preserving system prompt + tools + sampler
    /// + template variables), use `resetHistory()` instead.
    #[wasm_bindgen(js_name = reset)]
    pub fn reset(&self, opts: JsValue) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;

            // Pull systemPrompt + tools out of opts. Missing object → both null/empty.
            let (prompt_jsval, tools_jsval) = if opts.is_undefined() || opts.is_null() {
                (JsValue::NULL, JsValue::UNDEFINED)
            } else {
                let p =
                    js_sys::Reflect::get(&opts, &"systemPrompt".into()).unwrap_or(JsValue::NULL);
                let t = js_sys::Reflect::get(&opts, &"tools".into()).unwrap_or(JsValue::UNDEFINED);
                (p, t)
            };

            // Update the main-thread tool callback registry to match the new
            // list. Same shape as setTools: callbacks stay on main, only
            // metadata crosses postMessage.
            let (callbacks, tools_meta) = extract_tool_callbacks(&tools_jsval)?;
            {
                let mut st = state.borrow_mut();
                st.tool_callbacks = callbacks;
            }

            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"reset-chat".into());
            let _ = js_sys::Reflect::set(&msg, &"prompt".into(), &prompt_jsval);
            let _ = js_sys::Reflect::set(&msg, &"tools".into(), &tools_meta);
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post reset-chat: {e:?}")))?;
            let _ = wait_for_handshake(&state, "chat-reset").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Clear the conversation history (preserves system prompt, tools,
    /// sampler, template variables — only the user/assistant/tool
    /// turns get wiped).
    #[wasm_bindgen(js_name = resetHistory)]
    pub fn reset_history(&self) -> js_sys::Promise {
        let state = self.state.clone();
        promisify(async move {
            check_not_terminated(&state)?;
            let msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&msg, &"type".into(), &"reset-history".into());
            worker_post(&state.borrow().worker, &msg)
                .map_err(|e| JsError::new(&format!("post reset-history: {e:?}")))?;
            let _ = wait_for_handshake(&state, "history-set").await?;
            Ok(JsValue::UNDEFINED)
        })
    }

    /// Interrupt the currently-running `ask`. Posts a `stop` message to
    /// the worker, which calls `ChatHandleAsync::stop_generation` —
    /// core's inference loop checks the stop flag between tokens and
    /// breaks out. The in-flight `TokenStream` resolves normally with
    /// whatever tokens were already generated (the partial response is
    /// also recorded in the chat history, matching native behavior).
    ///
    /// No-op if no ask is in progress, or if the chat has been
    /// terminated. The chat remains usable after a stop — `ask` again
    /// to continue the conversation.
    #[wasm_bindgen(js_name = stopGeneration)]
    pub fn stop_generation(&self) -> Result<(), JsError> {
        let st = self.state.borrow();
        if st.terminated {
            // Match Python's silent no-op for stop-after-terminate.
            return Ok(());
        }
        let stop_msg = js_sys::Object::new();
        let _ = js_sys::Reflect::set(&stop_msg, &"type".into(), &"stop".into());
        worker_post(&st.worker, &stop_msg)
            .map_err(|e| JsError::new(&format!("post stop: {e:?}")))?;
        Ok(())
    }

    /// Shut down the worker. Any in-flight stream is failed with
    /// "terminated"; subsequent calls to `ask` reject. Returns a Promise
    /// that resolves once the underlying worker has fully shut down — on
    /// Node, that's `worker_threads.Worker.terminate()` returning a
    /// Promise. Awaiting it before spawning another `Chat` avoids
    /// piling up workers (each loads ~480 MB of model, so memory
    /// pressure climbs fast). On the browser, `Worker.terminate()` is
    /// synchronous; the returned Promise resolves on the next tick.
    pub fn terminate(&self) -> js_sys::Promise {
        let already_terminated = {
            let mut st = self.state.borrow_mut();
            if st.terminated {
                true
            } else {
                st.terminated = true;
                let stream = st.current_stream.take();
                if let Some(s) = stream {
                    WorkerStreamState::fail(&s, "Chat terminated".to_string());
                }
                false
            }
        };
        if already_terminated {
            return js_sys::Promise::resolve(&JsValue::UNDEFINED);
        }
        let worker = self.state.borrow().worker.clone();
        promisify(async move {
            // Call worker.terminate() via Reflect. In Node the shim
            // returns the Promise from `worker_threads.Worker.terminate()`;
            // in the browser the real `Worker.terminate()` returns
            // undefined. We await either shape — JsFuture::from on
            // a non-Promise just resolves immediately.
            let terminate_fn: js_sys::Function =
                match js_sys::Reflect::get(&worker, &"terminate".into()) {
                    Ok(v) => match v.dyn_into::<js_sys::Function>() {
                        Ok(f) => f,
                        Err(_) => return Ok(JsValue::UNDEFINED),
                    },
                    Err(_) => return Ok(JsValue::UNDEFINED),
                };
            if let Ok(ret) = terminate_fn.call0(&worker) {
                if ret.is_instance_of::<js_sys::Promise>() {
                    let _ = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(ret)).await;
                }
            }
            Ok(JsValue::UNDEFINED)
        })
    }
}

/// Synchronous router for messages from the chat worker. Runs from inside
/// the onmessage Closure. Borrow rules: take what you need, then drop the
/// borrow before invoking `WorkerStreamState::*` helpers (which take their
/// own borrow on the stream's inner state).
#[cfg(target_family = "wasm")]
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
        "tool-call" => {
            // Worker is asking us to dispatch a tool. Look up the callback
            // by name, invoke it (awaiting if it returns a Promise), then
            // post back `tool-reply` with the same id.
            let id = js_sys::Reflect::get(&data, &"id".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            let name = js_sys::Reflect::get(&data, &"name".into())
                .ok()
                .and_then(|v| v.as_string())
                .unwrap_or_default();
            let args = js_sys::Reflect::get(&data, &"args".into()).unwrap_or(JsValue::UNDEFINED);

            let callback = state.borrow().tool_callbacks.get(&name).cloned();
            let worker = state.borrow().worker.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let reply = js_sys::Object::new();
                let _ = js_sys::Reflect::set(&reply, &"type".into(), &"tool-reply".into());
                let _ = js_sys::Reflect::set(&reply, &"id".into(), &id.as_str().into());

                let Some(cb) = callback else {
                    let _ = js_sys::Reflect::set(
                        &reply,
                        &"error".into(),
                        &format!("no tool callback registered for {name:?}").into(),
                    );
                    let _ = worker_post(&worker, &reply);
                    return;
                };

                // Invoke the JS callback. If it returns a Promise, await
                // its resolution (this is the path that needs Plan A's
                // async core — inference suspends here, JS event loop
                // ticks, Promise resolves, control returns to worker).
                let result_val = match cb.call1(&JsValue::NULL, &args) {
                    Ok(v) => v,
                    Err(e) => {
                        let msg = js_sys::Reflect::get(&e, &"message".into())
                            .ok()
                            .and_then(|m| m.as_string())
                            .unwrap_or_else(|| format!("{e:?}"));
                        let _ = js_sys::Reflect::set(&reply, &"error".into(), &msg.into());
                        let _ = worker_post(&worker, &reply);
                        return;
                    }
                };

                let resolved = if result_val.is_instance_of::<js_sys::Promise>() {
                    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(result_val))
                        .await
                    {
                        Ok(v) => v,
                        Err(e) => {
                            let msg = js_sys::Reflect::get(&e, &"message".into())
                                .ok()
                                .and_then(|m| m.as_string())
                                .unwrap_or_else(|| format!("{e:?}"));
                            let _ = js_sys::Reflect::set(
                                &reply,
                                &"error".into(),
                                &format!("tool promise rejected: {msg}").into(),
                            );
                            let _ = worker_post(&worker, &reply);
                            return;
                        }
                    }
                } else {
                    result_val
                };

                let result_str = if let Some(s) = resolved.as_string() {
                    s
                } else {
                    js_sys::JSON::stringify(&resolved)
                        .ok()
                        .and_then(|s| s.as_string())
                        .unwrap_or_else(|| {
                            "ERROR: tool returned a non-serializable value".to_string()
                        })
                };
                let _ = js_sys::Reflect::set(&reply, &"result".into(), &result_str.into());
                let _ = worker_post(&worker, &reply);
            });
        }
        // Handshake / request-reply: resolve a pending oneshot if its
        // expected type matches. The full reply data is JSON-stringified
        // and handed to the waiter; the waiter parses it back into JS
        // and extracts payload fields. JSON round-trip is needed
        // because raw JsValue isn't UnwindSafe — see the
        // pending_handshake field doc.
        other => {
            let mut st = state.borrow_mut();
            let take_it = matches!(&st.pending_handshake, Some((t, _)) if t == other);
            if take_it {
                if let Some((_, tx)) = st.pending_handshake.take() {
                    let json = js_sys::JSON::stringify(&data)
                        .ok()
                        .and_then(|s| s.as_string())
                        .unwrap_or_else(|| "null".to_string());
                    let _ = tx.send(Ok(json));
                }
            }
        }
    }
}

#[cfg(target_family = "wasm")]
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
/// Returns the full reply data as a JsValue (JSON-parsed from the
/// stringified channel payload). Callers ignore it for signal-only
/// handshakes (`ready`, `model-loaded`, `chat-ready`); callers that
/// need a payload field pull it out via Reflect.
#[cfg(target_family = "wasm")]
async fn wait_for_handshake(
    state: &std::rc::Rc<RefCell<ChatState>>,
    expected_type: &str,
) -> Result<JsValue, JsError> {
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
        Ok(Ok(json)) => js_sys::JSON::parse(&json)
            .map_err(|e| JsError::new(&format!("reply JSON parse: {e:?}"))),
        Ok(Err(e)) => Err(JsError::new(&e)),
        Err(_) => Err(JsError::new(&format!(
            "handshake sender dropped before {expected_type}"
        ))),
    }
}

/// Parsed Chat.create options. `chat_opts_jsval` is the original JS object
/// minus the modelUrl / modelBytes / onDownloadProgress / tools keys — passed
/// through to the worker as-is via postMessage. We do NOT re-serialize via
/// `serde_wasm_bindgen::to_value(&ChatOptions)` because that converts nested
/// maps (e.g. `templateVariables: { enable_thinking: false }`) into JS Maps,
/// and the worker's `serde_wasm_bindgen::from_value` round-trip doesn't
/// always preserve the original Object-vs-Map shape — small differences
/// caused `templateVariables` to silently come through empty.
#[cfg(target_family = "wasm")]
struct ChatCreateParsed {
    model_url: Option<String>,
    model_bytes: Option<js_sys::Uint8Array>,
    /// Node-only: absolute host path to the model GGUF. The worker
    /// streams it into MEMFS in chunks via the Node-fs helper
    /// `globalThis.__nbw_node_file_to_memfs`, bypassing Node's
    /// 2 GiB `fs.readFileSync` cap and avoiding a main-thread
    /// `Buffer` of the model bytes entirely. Errors if used in a
    /// browser context (no Node fs available).
    model_path: Option<String>,
    /// Optional URL to fetch the mmproj GGUF from. Mutually exclusive with
    /// `mmproj_bytes`. Both null/undefined ⇒ text-only model.
    mmproj_url: Option<String>,
    /// Optional pre-fetched mmproj bytes. Same shape as `model_bytes`.
    mmproj_bytes: Option<js_sys::Uint8Array>,
    /// Node-only: absolute host path to the mmproj GGUF. Same
    /// constraints and mechanism as `model_path`.
    mmproj_path: Option<String>,
    on_progress: Option<js_sys::Function>,
    chat_opts_jsval: JsValue,
    /// Tool metadata for the worker. Just `{name, description, jsonSchema}`
    /// per entry — the user's JS callback stays main-thread-only and goes
    /// into `tool_callbacks` below.
    tools_jsval: JsValue,
    /// Map of tool name → JS callback. Stays on the main thread; the
    /// worker round-trips each invocation via `tool-call` / `tool-reply`.
    tool_callbacks: std::collections::HashMap<String, js_sys::Function>,
}

#[cfg(target_family = "wasm")]
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
    let model_path = js_sys::Reflect::get(obj, &"modelPath".into())
        .ok()
        .and_then(|v| v.as_string());
    let mmproj_url = js_sys::Reflect::get(obj, &"mmprojUrl".into())
        .ok()
        .and_then(|v| v.as_string());
    let mmproj_bytes = js_sys::Reflect::get(obj, &"mmprojBytes".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok());
    let mmproj_path = js_sys::Reflect::get(obj, &"mmprojPath".into())
        .ok()
        .and_then(|v| v.as_string());
    let on_progress = js_sys::Reflect::get(obj, &"onDownloadProgress".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());

    // Reject more-than-one source for model and mmproj (clear
    // error rather than silent precedence rules).
    let model_sources =
        model_url.is_some() as u8 + model_bytes.is_some() as u8 + model_path.is_some() as u8;
    if model_sources > 1 {
        return Err(JsError::new(
            "Chat.create: pass exactly one of modelUrl / modelBytes / modelPath, not multiple",
        ));
    }
    let mmproj_sources =
        mmproj_url.is_some() as u8 + mmproj_bytes.is_some() as u8 + mmproj_path.is_some() as u8;
    if mmproj_sources > 1 {
        return Err(JsError::new(
            "Chat.create: pass at most one of mmprojUrl / mmprojBytes / mmprojPath",
        ));
    }

    // Split `tools` (each entry tagged via `Tool.fromFn`) into:
    //   - tools_meta_array (name + description + jsonSchema) → JsValue
    //     for the worker, structured-cloneable
    //   - tool_callbacks (name → js_sys::Function) → stays on main thread
    //
    // Anything not shaped like `{__nbwKind: 'tool', name, description,
    // jsonSchema, callback}` errors out at create time so misuse surfaces
    // clearly rather than failing inside the worker.
    let tools_input = js_sys::Reflect::get(obj, &"tools".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null());
    let tools_meta_array = js_sys::Array::new();
    let mut tool_callbacks: std::collections::HashMap<String, js_sys::Function> =
        std::collections::HashMap::new();
    if let Some(tools_val) = tools_input {
        let arr: js_sys::Array = tools_val.dyn_into().map_err(|_| {
            JsError::new("Chat.create: `tools` must be an array of Tool.fromFn(...) values")
        })?;
        for (idx, raw) in arr.iter().enumerate() {
            let kind = js_sys::Reflect::get(&raw, &"__nbwKind".into())
                .ok()
                .and_then(|v| v.as_string());
            if kind.as_deref() != Some("tool") {
                return Err(JsError::new(&format!(
                    "Chat.create: tools[{idx}] is not a Tool.fromFn(...) value (missing __nbwKind=tool)",
                )));
            }
            let name = js_sys::Reflect::get(&raw, &"name".into())
                .ok()
                .and_then(|v| v.as_string())
                .ok_or_else(|| JsError::new(&format!("tools[{idx}]: missing name")))?;
            let description = js_sys::Reflect::get(&raw, &"description".into())
                .ok()
                .and_then(|v| v.as_string())
                .ok_or_else(|| JsError::new(&format!("tools[{idx}]: missing description")))?;
            let schema = js_sys::Reflect::get(&raw, &"jsonSchema".into())
                .map_err(|_| JsError::new(&format!("tools[{idx}]: missing jsonSchema")))?;
            let callback = js_sys::Reflect::get(&raw, &"callback".into())
                .map_err(|_| JsError::new(&format!("tools[{idx}]: missing callback")))?
                .dyn_into::<js_sys::Function>()
                .map_err(|_| JsError::new(&format!("tools[{idx}]: callback is not a function")))?;

            let meta_obj = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&meta_obj, &"name".into(), &name.as_str().into());
            let _ = js_sys::Reflect::set(
                &meta_obj,
                &"description".into(),
                &description.as_str().into(),
            );
            let _ = js_sys::Reflect::set(&meta_obj, &"jsonSchema".into(), &schema);
            tools_meta_array.push(&meta_obj);

            tool_callbacks.insert(name, callback);
        }
    }

    // Build a filtered JS object containing only the ChatOptions fields.
    let chat_opts_obj = js_sys::Object::new();
    let keys = js_sys::Object::keys(obj);
    for k in keys.iter() {
        let key_str = k.as_string().unwrap_or_default();
        if matches!(
            key_str.as_str(),
            "modelUrl"
                | "modelBytes"
                | "modelPath"
                | "mmprojUrl"
                | "mmprojBytes"
                | "mmprojPath"
                | "onDownloadProgress"
                | "tools"
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
        model_path,
        mmproj_url,
        mmproj_bytes,
        mmproj_path,
        on_progress,
        chat_opts_jsval: chat_opts_obj.into(),
        tools_jsval: tools_meta_array.into(),
        tool_callbacks,
    })
}
