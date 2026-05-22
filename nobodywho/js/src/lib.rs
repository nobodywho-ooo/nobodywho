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

// Force-import file-open syscalls into the wasm; see the module's
// doc-comment + js/build.rs.
#[cfg(target_arch = "wasm32")]
mod syscall_imports;

// Per-worker state for `runInWorker` — only used on wasm32 targets.
#[cfg(target_arch = "wasm32")]
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

// Streaming hook RAII removed: `nobodywho::llm::set_streaming_hook` was
// removed from core during the Emscripten migration. Without it, the worker
// can only post the full response at completion (no real-time per-token
// streaming). Restore HookRestore + askStreaming if/when the core API
// returns.

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
        let mmproj_vec: Option<Vec<u8>> = if mmproj_bytes.is_undefined()
            || mmproj_bytes.is_null()
        {
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
            let model = nobodywho::llm::get_model_from_bytes(
                &bytes,
                mmproj_path.as_deref(),
                0,
            )
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
    /// Build an audio prompt part from raw file bytes. Supported formats
    /// depend on the linked miniaudio decoder: WAV always; MP3/FLAC/Ogg
    /// when their `MA_NO_*` switches are off (note: the wasm-Emscripten
    /// build has playback/threading/engine cut out, but the decoder front
    /// is still in for WAV). The format is sniffed via the file header.
    ///
    /// Returns `{__nbwKind: 'audio', bytes: Uint8Array}`.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: Vec<u8>) -> js_sys::Object {
        make_media_part("audio", &bytes)
    }
}

/// Build a tagged media part object. `kind` is `"image"` or `"audio"`.
fn make_media_part(kind: &str, bytes: &[u8]) -> js_sys::Object {
    let o = js_sys::Object::new();
    let _ = js_sys::Reflect::set(&o, &"__nbwKind".into(), &JsValue::from_str(kind));
    let _ = js_sys::Reflect::set(
        &o,
        &"bytes".into(),
        &js_sys::Uint8Array::from(bytes).into(),
    );
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
#[cfg(target_arch = "wasm32")]
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
//     const chat = new Chat(model, { tools: [weather], systemPrompt: '...' });
//     const reply = await (await chat.ask('Weather in CPH?')).completed();
//
// v1 limitations (documented in README):
// - Only the in-process `Chat` accepts tools. `Chat` doesn't — JS
//   function refs can't survive postMessage and we don't yet have an RPC
//   bridge between worker and main thread to dispatch tool calls.
// - JS callbacks must be SYNCHRONOUS (return a string, not a Promise).
//   Core's tool-call dispatcher is `Fn(Value) -> String + Send + Sync`
//   and the wasm32 inference loop holds the single JS thread, so a
//   Promise returned from a callback would never resolve until inference
//   finishes — defeating the point. Async support needs core to grow an
//   `AsyncFn` variant of Tool, or for the dispatch to happen on a
//   separate worker. Tracked.

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
            .ok_or_else(|| {
                JsError::new("Tool.fromFn: jsonSchema must be JSON-serializable")
            })?;
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
    let arr = tools_val
        .dyn_ref::<js_sys::Array>()
        .ok_or_else(|| {
            JsError::new("Chat options.tools must be an array of Tool.fromFn(...) values")
        })?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let elem = arr.get(i);
        out.push(tool_from_tagged(&elem).map_err(|e| {
            JsError::new(&format!("Chat options.tools[{i}]: {e}"))
        })?);
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
        return Err(
            "not a Tool.fromFn(...) value — missing or wrong __nbwKind brand".to_string(),
        );
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
                    let ser = serde_wasm_bindgen::Serializer::new()
                        .serialize_maps_as_objects(true);
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
                    match wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(result)).await {
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
/// The wire format is stable; the grammar sampler runs through llguidance,
/// which needs a monotonic clock — Emscripten's libc has `clock_gettime`,
/// so this should work at runtime, but end-to-end is unverified.
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
/// new Chat(model, {
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
#[cfg(target_arch = "wasm32")]
fn worker_post(scope: &JsValue, msg: &JsValue) -> Result<(), JsValue> {
    let post_fn: js_sys::Function = js_sys::Reflect::get(scope, &"postMessage".into())?
        .dyn_into()
        .map_err(|_| JsValue::from_str("worker scope has no postMessage function"))?;
    post_fn.call1(scope, msg).map(|_| ())
}

/// Tool metadata sent across the worker boundary. The user's JS callback
/// stays on the main thread (function refs can't survive postMessage); the
/// worker just sees this metadata and synthesizes an RPC stub.
#[cfg(target_arch = "wasm32")]
#[derive(serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct ToolMeta {
    name: String,
    description: String,
    json_schema: serde_json::Value,
}

#[cfg(target_arch = "wasm32")]
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
#[cfg(target_arch = "wasm32")]
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
        let data =
            js_sys::Reflect::get(&evt, &"data".into()).unwrap_or(JsValue::UNDEFINED);
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
#[cfg(target_arch = "wasm32")]
async fn handle_worker_message(
    data: JsValue,
    scope: &JsValue,
) -> Result<(), String> {
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
            let bytes_val = js_sys::Reflect::get(&data, &"bytes".into())
                .map_err(|_| "missing 'bytes' field".to_string())?;
            let u8_array: js_sys::Uint8Array = bytes_val
                .dyn_into()
                .map_err(|_| "'bytes' must be a Uint8Array".to_string())?;
            let bytes = u8_array.to_vec();

            // Optional mmproj-bytes — write to MEMFS via the JS-side
            // FS.writeFile and pass the path to core. Field is missing/
            // undefined for text-only loads.
            let mmproj_path = match js_sys::Reflect::get(&data, &"mmprojBytes".into())
                .ok()
                .filter(|v| !v.is_undefined() && !v.is_null())
                .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok())
            {
                Some(u8a) => Some(write_bytes_to_memfs("mmproj", &u8a.to_vec())?),
                None => None,
            };

            let model = nobodywho::llm::get_model_from_bytes(
                &bytes,
                mmproj_path.as_deref(),
                0,
            )
            .map_err(|e| e.to_string())?;
            WORKER_MODEL.with(|m| *m.borrow_mut() = Some(Arc::new(model)));
            post(&worker_reply("model-loaded"));
        }
        "create-chat" => {
            let options = js_sys::Reflect::get(&data, &"options".into())
                .unwrap_or(JsValue::UNDEFINED);
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
            let tools_jsval = js_sys::Reflect::get(&data, &"tools".into())
                .unwrap_or(JsValue::UNDEFINED);
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
            let sender =
                PENDING_TOOL_CALLS.with(|m| m.borrow_mut().remove(&id));
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

            // Run inference to completion, then post the full response as a
            // single "token" message followed by "ask-done". Without
            // `set_streaming_hook` in core we can't deliver tokens in real
            // time — inference blocks the worker thread for its full
            // duration, then the result is delivered in one chunk. The
            // worker is still off the main thread, so the page stays
            // responsive.
            let mut stream = handle.ask(prompt);
            let full = stream.completed().await.map_err(|e| e.to_string())?;
            let payload = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&payload, &"type".into(), &"token".into());
            let _ = js_sys::Reflect::set(&payload, &"token".into(), &full.as_str().into());
            post(&payload);
            post(&worker_reply("ask-done"));
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
#[cfg(target_arch = "wasm32")]
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
                    let ser = serde_wasm_bindgen::Serializer::new()
                        .serialize_maps_as_objects(true);
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
#[cfg(target_arch = "wasm32")]
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

// ---------- TokenStream ----------
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
pub struct TokenStream {
    state: std::rc::Rc<RefCell<WorkerStreamState>>,
}

#[cfg(target_arch = "wasm32")]
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
    /// The spawned worker as an opaque JS handle. Reached via Reflect for
    /// postMessage / terminate / onmessage / onerror, so the same code
    /// works whether the underlying object is a real browser `Worker` or
    /// the Node shim wrapping a `worker_threads.Worker` (see pre.js's
    /// `__nbw_spawn_worker`).
    worker: JsValue,
    current_stream: Option<std::rc::Rc<RefCell<WorkerStreamState>>>,
    /// While `Chat::create` is running through its load-model / create-chat
    /// handshake, this holds `(expected_reply_type, sender)`. The onmessage
    /// closure resolves the sender when a message of that type arrives.
    pending_handshake:
        Option<(String, tokio::sync::oneshot::Sender<Result<(), String>>)>,
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
#[cfg(target_arch = "wasm32")]
fn worker_terminate(worker: &JsValue) {
    if let Ok(f) = js_sys::Reflect::get(worker, &"terminate".into()) {
        if let Ok(fun) = f.dyn_into::<js_sys::Function>() {
            let _ = fun.call0(worker);
        }
    }
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
            worker_terminate(&st.worker);
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
            let worker: JsValue = wasm_bindgen_futures::JsFuture::from(
                js_sys::Promise::from(worker_promise),
            )
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

            let state_weak = std::rc::Rc::downgrade(&state);
            let on_message =
                wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
                    if let Some(state) = state_weak.upgrade() {
                        // `evt` is either a browser MessageEvent (with `.data`)
                        // or the Node shim's `{ data }` plain object — both
                        // respond to Reflect-get('data').
                        let data = js_sys::Reflect::get(&evt, &"data".into())
                            .unwrap_or(JsValue::UNDEFINED);
                        handle_chat_message(&state, data);
                    }
                });

            let state_weak2 = std::rc::Rc::downgrade(&state);
            let on_error =
                wasm_bindgen::closure::Closure::<dyn FnMut(JsValue)>::new(move |evt: JsValue| {
                    if let Some(state) = state_weak2.upgrade() {
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

            // Handshake step 2: get the model bytes, post 'load-model'.
            // Model bytes come from modelBytes or modelUrl; mmproj is
            // optional and follows the same shape (mmprojBytes / mmprojUrl).
            // The progress callback is shared between both downloads.
            let model_bytes_value: JsValue = if let Some(bytes) = parsed.model_bytes {
                bytes.into()
            } else if let Some(url) = parsed.model_url {
                let bytes_promise = fetch_model_bytes(url, parsed.on_progress.clone());
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

            let mmproj_bytes_value: Option<JsValue> = if let Some(bytes) = parsed.mmproj_bytes {
                Some(bytes.into())
            } else if let Some(url) = parsed.mmproj_url {
                let bytes_promise = fetch_model_bytes(url, parsed.on_progress);
                Some(
                    wasm_bindgen_futures::JsFuture::from(bytes_promise)
                        .await
                        .map_err(|e| {
                            let msg = js_sys::Reflect::get(&e, &"message".into())
                                .ok()
                                .and_then(|m| m.as_string())
                                .unwrap_or_else(|| format!("{e:?}"));
                            JsError::new(&format!("fetchModelBytes(mmproj): {msg}"))
                        })?,
                )
            } else {
                None
            };

            let load_msg = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&load_msg, &"type".into(), &"load-model".into());
            let _ = js_sys::Reflect::set(&load_msg, &"bytes".into(), &model_bytes_value);
            if let Some(mmproj) = mmproj_bytes_value {
                let _ = js_sys::Reflect::set(&load_msg, &"mmprojBytes".into(), &mmproj);
            }
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
            let _ =
                js_sys::Reflect::set(&create_msg, &"type".into(), &"create-chat".into());
            let _ =
                js_sys::Reflect::set(&create_msg, &"options".into(), &parsed.chat_opts_jsval);
            let _ = js_sys::Reflect::set(&create_msg, &"tools".into(), &tools_jsval);
            worker_post(&state.borrow().worker, &create_msg)
                .map_err(|e| JsError::new(&format!("post create-chat: {e:?}")))?;
            wait_for_handshake(&state, "chat-ready").await?;

            Ok(Chat { state })
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
        worker_post(&st.worker, &ask_msg)
            .map_err(|e| JsError::new(&format!("post ask: {e:?}")))?;
        drop(st);

        Ok(TokenStream {
            state: stream_state,
        })
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
            let args = js_sys::Reflect::get(&data, &"args".into())
                .unwrap_or(JsValue::UNDEFINED);

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
/// minus the modelUrl / modelBytes / onDownloadProgress / tools keys — passed
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
    /// Optional URL to fetch the mmproj GGUF from. Mutually exclusive with
    /// `mmproj_bytes`. Both null/undefined ⇒ text-only model.
    mmproj_url: Option<String>,
    /// Optional pre-fetched mmproj bytes. Same shape as `model_bytes`.
    mmproj_bytes: Option<js_sys::Uint8Array>,
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
    let mmproj_url = js_sys::Reflect::get(obj, &"mmprojUrl".into())
        .ok()
        .and_then(|v| v.as_string());
    let mmproj_bytes = js_sys::Reflect::get(obj, &"mmprojBytes".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<js_sys::Uint8Array>().ok());
    let on_progress = js_sys::Reflect::get(obj, &"onDownloadProgress".into())
        .ok()
        .filter(|v| !v.is_undefined() && !v.is_null())
        .and_then(|v| v.dyn_into::<js_sys::Function>().ok());

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
                .map_err(|_| {
                    JsError::new(&format!("tools[{idx}]: callback is not a function"))
                })?;

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
                | "mmprojUrl"
                | "mmprojBytes"
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
        mmproj_url,
        mmproj_bytes,
        on_progress,
        chat_opts_jsval: chat_opts_obj.into(),
        tools_jsval: tools_meta_array.into(),
        tool_callbacks,
    })
}
