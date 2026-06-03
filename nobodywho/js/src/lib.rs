//! WebAssembly binding for NobodyWho — mirrors the Python binding's API for
//! JS/TS. Emscripten pthreads run inference on a background thread via
//! `std::thread::spawn`, same as native. Build steps in `README.md`.
//!

// Native builds flag wasm-only items as dead code (their wasm_bindgen callers
// are cfg'd to wasm). Suppress on native only; wasm keeps real dead-code checks.
#![cfg_attr(not(target_family = "wasm"), allow(unused))]
// wasm_bindgen's `static_method_of` macro emits an unsuppressable
// `unused_variables` warning from inside its expansion; CI's `-D warnings`
// would make it fatal, so allow it crate-wide.
#![allow(unused_variables)]

//! Methods return `js_sys::Promise` rather than `pub async fn`: wasm_bindgen's
//! async needs an `UnwindSafe` future, which several of our types
//! (`tokio::sync::Mutex`/`Receiver`) aren't. Each is a plain `pub fn` whose body
//! runs through [`promisify`] (`AssertUnwindSafe`).

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;

#[cfg(target_family = "wasm")]
use std::cell::RefCell;

use wasm_bindgen::prelude::*;

// Force-import file-open syscalls into the wasm; see the module's
// doc-comment + js/build.rs.
#[cfg(target_family = "wasm")]
mod syscall_imports;

/// No-op override of libc's `__cxa_atexit`: a libc++ global-dtor's wasm
/// signature mismatches `__funcs_on_exit`, trapping the first export call.
/// Dropping the registration avoids it — dtors won't run at shutdown, which is
/// fine (the instance lives the whole process). `#[no_mangle]` shadows the sysroot.
///
/// # Safety: ignores its args, returns success; only drops atexit registrations.
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

/// Handle to swap the global tracing level filter at runtime (see
/// [`set_log_level`]). Set once by [`init`]; the type is the reload layer's
/// handle over the registry it filters.
#[cfg(target_family = "wasm")]
static LOG_RELOAD: std::sync::OnceLock<
    tracing_subscriber::reload::Handle<
        tracing_subscriber::filter::LevelFilter,
        tracing_subscriber::Registry,
    >,
> = std::sync::OnceLock::new();

/// Install panic hook and tracing subscriber.
/// Auto-called via the postRun hook in pre.js — no need to call from JS.
#[wasm_bindgen(js_name = init)]
pub fn init() {
    console_error_panic_hook::set_once();
    #[cfg(target_family = "wasm")]
    {
        use tracing_subscriber::prelude::*;
        // Build the subscriber manually (not tracing_wasm's global default) so a
        // *reloadable* level filter sits in front of the WASMLayer. Default WARN
        // keeps the console quiet; `setLogLevel(...)` dials it up at runtime.
        let (filter, handle) =
            tracing_subscriber::reload::Layer::new(tracing_subscriber::filter::LevelFilter::WARN);
        let _ = LOG_RELOAD.set(handle);
        let _ = tracing_subscriber::registry()
            .with(filter)
            .with(tracing_wasm::WASMLayer::new(
                tracing_wasm::WASMLayerConfigBuilder::new().build(),
            ))
            .try_init();
        // Route llama.cpp/ggml logs through `tracing` (→ leveled console.*)
        // instead of stderr, which Emscripten dumps to console.error — making
        // every info line look like an error. Matches Python/Flutter.
        nobodywho::send_llamacpp_logs_to_tracing();
    }
}

/// Set the console log verbosity at runtime: one of
/// `off | error | warn | info | debug | trace` (default `warn`).
/// The JS analog of Python's `logging.basicConfig(level=...)`.
#[cfg(target_family = "wasm")]
#[wasm_bindgen(js_name = setLogLevel)]
pub fn set_log_level(level: &str) -> Result<(), JsError> {
    use tracing_subscriber::filter::LevelFilter;
    let lvl = match level.to_ascii_lowercase().as_str() {
        "off" => LevelFilter::OFF,
        "error" => LevelFilter::ERROR,
        "warn" => LevelFilter::WARN,
        "info" => LevelFilter::INFO,
        "debug" => LevelFilter::DEBUG,
        "trace" => LevelFilter::TRACE,
        other => {
            return Err(JsError::new(&format!(
                "invalid log level '{other}': expected off|error|warn|info|debug|trace"
            )))
        }
    };
    LOG_RELOAD
        .get()
        .ok_or_else(|| JsError::new("logging not initialized"))?
        .reload(lvl)
        .map_err(|e| JsError::new(&e.to_string()))?;
    Ok(())
}

// ---------- Promise helper ----------

/// Cosine similarity between two embedding vectors (mirrors Python's
/// `cosine_similarity`). Accepts `Float32Array | number[]`, throws on length
/// mismatch, returns NaN if either has zero magnitude.
#[wasm_bindgen(js_name = cosineSimilarity)]
pub fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> Result<f32, JsError> {
    if a.len() != b.len() {
        return Err(JsError::new(&format!(
            "cosineSimilarity: vectors have different lengths ({} vs {})",
            a.len(),
            b.len()
        )));
    }
    Ok(nobodywho::encoder::cosine_similarity(&a, &b))
}

/// RAII guard that keeps the JS event loop pumping while held (acquires/releases
/// a ref-counted keepalive timer from pre.js). Inference runs on an Emscripten
/// pthread whose cross-thread token wakes only arrive while the main loop ticks.
#[cfg(target_family = "wasm")]
struct KeepAlive;

#[cfg(target_family = "wasm")]
impl KeepAlive {
    fn new() -> Self {
        if let Ok(f) = js_sys::Reflect::get(&js_sys::global(), &"__nbw_keepalive_acquire".into()) {
            if let Ok(f) = f.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&JsValue::NULL);
            }
        }
        KeepAlive
    }
}

#[cfg(target_family = "wasm")]
impl Drop for KeepAlive {
    fn drop(&mut self) {
        if let Ok(f) = js_sys::Reflect::get(&js_sys::global(), &"__nbw_keepalive_release".into()) {
            if let Ok(f) = f.dyn_into::<js_sys::Function>() {
                let _ = f.call0(&JsValue::NULL);
            }
        }
    }
}

/// Wrap a `Future<Output = Result<T, JsError>>` into a `js_sys::Promise`,
/// asserting it's unwind-safe and catching panics so they reject the promise
/// rather than tearing down the whole wasm instance.
fn promisify<F, T>(fut: F) -> js_sys::Promise
where
    F: Future<Output = Result<T, JsError>> + 'static,
    T: Into<JsValue>,
{
    // AssertUnwindSafe satisfies future_to_promise's UnwindSafe bound. A Rust
    // panic propagates as a hard wasm abort (like a C++ exception crossing the
    // boundary on Emscripten).
    wasm_bindgen_futures::future_to_promise(AssertUnwindSafe(async move {
        // Keep the event loop pumping for the lifetime of this future so
        // cross-thread inference wakes are delivered (see KeepAlive).
        #[cfg(target_family = "wasm")]
        let _keepalive = KeepAlive::new();
        match fut.await {
            Ok(v) => Ok(v.into()),
            Err(e) => Err(JsValue::from(e)),
        }
    }))
}

// ---------- Model ----------

/// A loaded GGUF model. Share between `Chat` and `Encoder` instances; the
/// underlying model data is reference-counted.
///
/// Load via `Model.load({ modelUrl })` (browser — cached via Cache API)
/// or `Model.load({ modelPath })` (Node — reads from host disk via NODEFS).
#[wasm_bindgen]
pub struct Model {
    inner: Arc<nobodywho::llm::Model>,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl Model {
    /// Load a GGUF model from a URL or host filesystem path.
    ///
    /// ```js
    /// // Browser — fetched + cached via Cache API:
    /// const model = await Model.load({ modelUrl: 'https://...' });
    /// // Node — reads from disk via NODEFS:
    /// const model = await Model.load({ modelPath: '/path/to/model.gguf' });
    /// // Multimodal (vision/audio) — pass mmproj too:
    /// const model = await Model.load({ modelUrl: '...', mmprojUrl: '...' });
    /// // Track download progress on URL loads (kind: 'model' | 'mmproj';
    /// // total is 0 if the server sent no Content-Length):
    /// await Model.load({ modelUrl: '...', onProgress: (loaded, total, kind) =>
    ///   console.log(`${kind} ${loaded}/${total}`) });
    /// ```
    #[wasm_bindgen(js_name = load)]
    pub fn load(opts: &JsValue) -> js_sys::Promise {
        let obj = match opts.dyn_ref::<js_sys::Object>() {
            Some(o) => o.clone(),
            None => {
                return js_sys::Promise::reject(
                    &JsError::new("Model.load requires an options object").into(),
                )
            }
        };

        let model_url = js_sys::Reflect::get(&obj, &"modelUrl".into())
            .ok()
            .and_then(|v| v.as_string());
        let model_path = js_sys::Reflect::get(&obj, &"modelPath".into())
            .ok()
            .and_then(|v| v.as_string());
        let mmproj_url = js_sys::Reflect::get(&obj, &"mmprojUrl".into())
            .ok()
            .and_then(|v| v.as_string());
        let mmproj_path = js_sys::Reflect::get(&obj, &"mmprojPath".into())
            .ok()
            .and_then(|v| v.as_string());
        // Optional progress callback: onProgress(loaded, total, kind) per chunk
        // (kind "model"|"mmproj", total 0 if no Content-Length). URL loads only.
        let on_progress = js_sys::Reflect::get(&obj, &"onProgress".into())
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Function>().ok());

        if model_url.is_none() && model_path.is_none() {
            return js_sys::Promise::reject(
                &JsError::new("Model.load: pass modelUrl or modelPath").into(),
            );
        }

        promisify(async move {
            let model_memfs: std::path::PathBuf = if let Some(path) = model_path {
                mount_host_path_via_nodefs("model", &path).map_err(|e| JsError::new(&e))?
            } else if let Some(url) = model_url {
                stream_url_to_memfs("model", &url, on_progress.as_ref())
                    .await
                    .map_err(|e| JsError::new(&e))?
            } else {
                unreachable!()
            };

            let mmproj_memfs: Option<std::path::PathBuf> = if let Some(path) = mmproj_path {
                Some(mount_host_path_via_nodefs("mmproj", &path).map_err(|e| JsError::new(&e))?)
            } else if let Some(url) = mmproj_url {
                Some(
                    stream_url_to_memfs("mmproj", &url, on_progress.as_ref())
                        .await
                        .map_err(|e| JsError::new(&e))?,
                )
            } else {
                None
            };

            let mut model =
                nobodywho::llm::get_model_from_path(&model_memfs, mmproj_memfs.as_deref(), 0)
                    .map_err(|e| JsError::new(&nobodywho::render_miette(&e)))?;
            // Hand the model its mmap backing buffer(s) so they free on drop.
            for (ptr, size) in take_pending_model_buffers() {
                model.attach_backing_buffer(ptr, size);
            }

            Ok(Model {
                inner: Arc::new(model),
            })
        })
    }
}

// ---------- Multimodal: Image / Audio / prompt assembly ----------
//
// `Chat.ask` takes a string or an array of `string | Image | Audio`.
// `Image/Audio.fromBytes` return tagged `{__nbwKind, bytes}` objects; the Rust
// side pushes the bytes (`Prompt::push_media_bytes`) to decode in-memory
// (`MtmdBitmap::from_buffer`) — the inference pthread can't read the main
// thread's MEMFS, and a browser has no fs anyway.

/// Image factory namespace for multimodal prompts. `fromBytes` works in the
/// browser and Node; `fromPath` is Node-only (mirrors Python's `Image("/path")`).
#[wasm_bindgen]
pub struct Image;

#[wasm_bindgen]
impl Image {
    /// Build an image prompt part by reading a host file (Node-only; in the
    /// browser, fetch the bytes and use `fromBytes()`). Async because the Node
    /// fs lookup uses `await import('node:fs')`. Mirrors Python's `Image(path)`.
    ///
    /// ```js
    /// const img = await Image.fromPath('/path/to/dog.png');
    /// ```
    #[wasm_bindgen(js_name = fromPath)]
    pub fn from_path(path: String) -> js_sys::Promise {
        media_from_path("image", path)
    }

    /// Build an image prompt part from raw file bytes (JPEG/PNG/BMP/GIF/TGA/
    /// PSD/PIC/PNM — anything `stb_image` decodes; format sniffed from the
    /// header by mtmd).
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
    /// Build an audio prompt part by reading a host file (Node-only; in the
    /// browser, fetch the bytes and use `fromBytes()`). Async because the Node
    /// fs lookup uses `await import('node:fs')`. Mirrors Python's `Audio(path)`.
    ///
    /// ```js
    /// const audio = await Audio.fromPath('/path/to/foo.wav');
    /// ```
    #[wasm_bindgen(js_name = fromPath)]
    pub fn from_path(path: String) -> js_sys::Promise {
        media_from_path("audio", path)
    }

    /// Build an audio prompt part from raw file bytes. wasm build supports
    /// WAV/MP3/FLAC (miniaudio's decoders; playback/engine cut via `MA_NO_*`).
    /// Format sniffed from the header by mtmd's `is_audio_file`.
    ///
    /// Returns `{__nbwKind: 'audio', bytes: Uint8Array}`.
    #[wasm_bindgen(js_name = fromBytes)]
    pub fn from_bytes(bytes: Vec<u8>) -> js_sys::Object {
        make_media_part("audio", &bytes)
    }
}

/// A completed sampler configuration. Created via `SamplerBuilder` or
/// `SamplerPresets`. Pass to `Chat.create({sampler: ...})`. Mirrors
/// Python's `SamplerConfig`.
#[wasm_bindgen]
pub struct SamplerConfig {
    inner: nobodywho::sampler_config::SamplerConfig,
}

#[wasm_bindgen]
impl SamplerConfig {
    /// Serialize to a JSON string.
    #[wasm_bindgen(js_name = toJSON)]
    pub fn to_json(&self) -> Result<String, JsError> {
        serde_json::to_string(&self.inner).map_err(|e| JsError::new(&e.to_string()))
    }

    /// Deserialize from a JSON string.
    #[wasm_bindgen(js_name = fromJSON)]
    pub fn from_json(json: &str) -> Result<SamplerConfig, JsError> {
        let inner: nobodywho::sampler_config::SamplerConfig =
            serde_json::from_str(json).map_err(|e| JsError::new(&e.to_string()))?;
        Ok(SamplerConfig { inner })
    }
}

/// Static factory for common sampler presets. Mirrors Python's
/// `SamplerPresets`.
///
/// ```js
/// await Chat.create({ modelUrl, sampler: SamplerPresets.greedy() });
/// await Chat.create({ modelUrl, sampler: SamplerPresets.temperature(0.8) });
/// await Chat.create({ modelUrl, sampler: SamplerPresets.constrainWithJsonSchema({type: "object"}) });
/// ```
#[wasm_bindgen]
pub struct SamplerPresets;

#[wasm_bindgen]
impl SamplerPresets {
    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = default)]
    pub fn default_preset() -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerConfig::default(),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn greedy() -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::greedy(),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::temperature(temperature),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = topK)]
    pub fn top_k(top_k: i32) -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::top_k(top_k),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = topP)]
    pub fn top_p(top_p: f32) -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::top_p(top_p),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = constrainWithRegex)]
    pub fn constrain_with_regex(pattern: String) -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::constrain_with_regex(pattern),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = constrainWithJsonSchema)]
    pub fn constrain_with_json_schema(schema: JsValue) -> Result<SamplerConfig, JsError> {
        let schema_str = if schema.is_string() {
            schema.as_string().unwrap()
        } else {
            js_sys::JSON::stringify(&schema)
                .map_err(|_| JsError::new("failed to stringify JSON schema"))?
                .as_string()
                .unwrap_or_default()
        };
        Ok(SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::constrain_with_json_schema(
                schema_str,
            ),
        })
    }

    #[wasm_bindgen(static_method_of = SamplerPresets, js_name = constrainWithGrammar)]
    pub fn constrain_with_grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::constrain_with_grammar(grammar),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn dry() -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::dry(),
        }
    }

    #[wasm_bindgen(static_method_of = SamplerPresets)]
    pub fn json() -> SamplerConfig {
        SamplerConfig {
            inner: nobodywho::sampler_config::SamplerPresets::json(),
        }
    }
}

/// Fluent builder for sampler chains. Mirrors Python's `SamplerBuilder`.
///
/// ```js
/// const sampler = new SamplerBuilder()
///   .topK(40)
///   .topP(0.95)
///   .temperature(0.7)
///   .dist();
/// await Chat.create({ modelUrl, sampler });
/// ```
#[wasm_bindgen]
pub struct SamplerBuilder {
    inner: nobodywho::sampler_config::SamplerConfig,
}

fn shift(builder: SamplerBuilder, step: nobodywho::sampler_config::ShiftStep) -> SamplerBuilder {
    SamplerBuilder {
        inner: builder.inner.shift(step),
    }
}

fn sample(builder: SamplerBuilder, step: nobodywho::sampler_config::SampleStep) -> SamplerConfig {
    SamplerConfig {
        inner: builder.inner.sample(step),
    }
}

#[wasm_bindgen]
impl SamplerBuilder {
    #[wasm_bindgen(constructor)]
    pub fn new() -> SamplerBuilder {
        SamplerBuilder {
            inner: nobodywho::sampler_config::SamplerConfig::new(),
        }
    }

    pub fn temperature(self, temperature: f32) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::Temperature { temperature },
        )
    }

    #[wasm_bindgen(js_name = topK)]
    pub fn top_k(self, top_k: i32) -> SamplerBuilder {
        shift(self, nobodywho::sampler_config::ShiftStep::TopK { top_k })
    }

    #[wasm_bindgen(js_name = topP)]
    pub fn top_p(self, top_p: f32, min_keep: u32) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::TopP { top_p, min_keep },
        )
    }

    #[wasm_bindgen(js_name = minP)]
    pub fn min_p(self, min_p: f32, min_keep: u32) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::MinP { min_p, min_keep },
        )
    }

    pub fn penalties(
        self,
        penalty_repeat: f32,
        penalty_last_n: i32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            },
        )
    }

    pub fn dry(
        self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            },
        )
    }

    pub fn xtc(self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            },
        )
    }

    #[wasm_bindgen(js_name = typicalP)]
    pub fn typical_p(self, typ_p: f32, min_keep: u32) -> SamplerBuilder {
        shift(
            self,
            nobodywho::sampler_config::ShiftStep::TypicalP { typ_p, min_keep },
        )
    }

    pub fn dist(self) -> SamplerConfig {
        sample(self, nobodywho::sampler_config::SampleStep::Dist)
    }

    pub fn greedy(self) -> SamplerConfig {
        sample(self, nobodywho::sampler_config::SampleStep::Greedy)
    }

    #[wasm_bindgen(js_name = mirostatV1)]
    pub fn mirostat_v1(self, tau: f32, eta: f32, m: i32) -> SamplerConfig {
        sample(
            self,
            nobodywho::sampler_config::SampleStep::MirostatV1 { tau, eta, m },
        )
    }

    #[wasm_bindgen(js_name = mirostatV2)]
    pub fn mirostat_v2(self, tau: f32, eta: f32) -> SamplerConfig {
        sample(
            self,
            nobodywho::sampler_config::SampleStep::MirostatV2 { tau, eta },
        )
    }
}

/// Read a host file into a `Vec<u8>` via the Node-only pre.js helper
/// `globalThis.__nbw_node_read_file`. Errors with browser guidance if absent.
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

/// Shared body for `Image.fromPath` / `Audio.fromPath`: read a host file
/// (Node-only, via `read_node_file_bytes`) and wrap its bytes as a tagged
/// media part of `kind`. Returns a Promise that rejects on non-wasm targets —
/// there's no Node fs there, but the crate must still compile for the host so
/// the `lint` CI job can run `cargo test`.
fn media_from_path(kind: &'static str, path: String) -> js_sys::Promise {
    promisify(async move {
        #[cfg(target_family = "wasm")]
        {
            let bytes = read_node_file_bytes(&path).await?;
            Ok(JsValue::from(make_media_part(kind, &bytes)))
        }
        #[cfg(not(target_family = "wasm"))]
        {
            let _ = (kind, path);
            // Type-annotate so `promisify`'s `T: Into<JsValue>` bound can be
            // inferred on native — the Err-only branch can't on its own.
            Err::<JsValue, _>(JsError::new("fromPath: not supported on this target"))
        }
    })
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

/// Mount the host dir of `src_path` via NODEFS and return a virtual-FS path
/// llama.cpp can `fopen` directly — `fread` streams from host disk into wasm
/// tensor allocations with no MEMFS copy. Node-only; errors in browser.
#[cfg(target_family = "wasm")]
fn mount_host_path_via_nodefs(kind: &str, src_path: &str) -> Result<std::path::PathBuf, String> {
    let path = std::path::Path::new(src_path);
    let dir = path
        .parent()
        .ok_or("mount_nodefs: path has no parent directory")?
        .to_str()
        .ok_or("mount_nodefs: non-UTF-8 path")?;
    let filename = path
        .file_name()
        .ok_or("mount_nodefs: path has no filename")?
        .to_str()
        .ok_or("mount_nodefs: non-UTF-8 filename")?;

    let mountpoint = format!("/mnt/nbw-{kind}");

    let global = js_sys::global();
    let helper = js_sys::Reflect::get(&global, &"__nbw_mount_nodefs".into())
        .map_err(|_| "mount_nodefs: lookup failed".to_string())?;
    if helper.is_undefined() || helper.is_null() {
        return Err(
            "modelPath/mmprojPath is Node-only; in browser use modelUrl or mmprojUrl".to_string(),
        );
    }
    let helper_fn: js_sys::Function = helper
        .dyn_into()
        .map_err(|_| "__nbw_mount_nodefs is not a function".to_string())?;
    helper_fn
        .call2(
            &JsValue::NULL,
            &JsValue::from_str(dir),
            &JsValue::from_str(&mountpoint),
        )
        .map_err(|e| {
            let msg = js_sys::Reflect::get(&e, &"message".into())
                .ok()
                .and_then(|m| m.as_string())
                .unwrap_or_else(|| format!("{e:?}"));
            format!("__nbw_mount_nodefs failed: {msg}")
        })?;

    Ok(std::path::PathBuf::from(format!("{mountpoint}/{filename}")))
}

/// Resolve a `ReadableStreamDefaultReader` from a body and stream it
/// into a MEMFS file via `FS.open` / `FS.write` / `FS.close`.
#[cfg(target_family = "wasm")]
async fn stream_reader_to_memfs(
    reader: &web_sys::ReadableStreamDefaultReader,
    memfs_path: &str,
    total: f64,
    kind: &str,
    on_progress: Option<&js_sys::Function>,
) -> Result<(), String> {
    let fs = {
        let global_obj = js_sys::global();
        let module = js_sys::Reflect::get(&global_obj, &"Module".into())
            .map_err(|_| "Module not found".to_string())?;
        js_sys::Reflect::get(&module, &"FS".into())
            .map_err(|_| "Module.FS not found".to_string())?
    };
    let fs_open: js_sys::Function = js_sys::Reflect::get(&fs, &"open".into())
        .ok()
        .and_then(|v| v.dyn_into().ok())
        .ok_or("FS.open not found")?;
    let fs_write: js_sys::Function = js_sys::Reflect::get(&fs, &"write".into())
        .ok()
        .and_then(|v| v.dyn_into().ok())
        .ok_or("FS.write not found")?;
    let fs_close: js_sys::Function = js_sys::Reflect::get(&fs, &"close".into())
        .ok()
        .and_then(|v| v.dyn_into().ok())
        .ok_or("FS.close not found")?;

    let stream = fs_open
        .call2(&fs, &JsValue::from_str(memfs_path), &JsValue::from_str("w"))
        .map_err(|e| format!("FS.open({memfs_path}, w) failed: {e:?}"))?;

    let mut downloaded: f64 = 0.0;
    loop {
        let read_result = wasm_bindgen_futures::JsFuture::from(reader.read())
            .await
            .map_err(|e| format!("reader.read(): {e:?}"))?;
        let done = js_sys::Reflect::get(&read_result, &"done".into())
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&read_result, &"value".into())
            .map_err(|_| "read result missing 'value'".to_string())?;
        let chunk: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| "read chunk is not a Uint8Array".to_string())?;
        let chunk_len = chunk.byte_length() as f64;
        let args = js_sys::Array::of4(
            &stream,
            &chunk.into(),
            &JsValue::from_f64(0.0),
            &JsValue::from_f64(chunk_len),
        );
        let written = js_sys::Reflect::apply(&fs_write, &fs, &args)
            .map_err(|e| format!("FS.write failed: {e:?}"))?
            .as_f64()
            .unwrap_or(0.0);
        if (written - chunk_len).abs() > 0.5 {
            let _ = fs_close.call1(&fs, &stream);
            return Err(format!(
                "FS.write short write: {written}/{chunk_len} at offset {downloaded}"
            ));
        }
        downloaded += chunk_len;
        if let Some(cb) = on_progress {
            let _ = cb.call3(
                &JsValue::null(),
                &JsValue::from_f64(downloaded),
                &JsValue::from_f64(total),
                &JsValue::from_str(kind),
            );
        }
    }
    let _ = fs_close.call1(&fs, &stream);
    Ok(())
}

#[cfg(target_family = "wasm")]
thread_local! {
    /// `(ptr, size)` buffers streamed into wasm memory, awaiting attachment to
    /// the `Model` that mmaps them; drained after `get_model_from_path`.
    static PENDING_MODEL_BUFFERS: std::cell::RefCell<Vec<(usize, usize)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Drain the buffers streamed since the last call (see `PENDING_MODEL_BUFFERS`).
#[cfg(target_family = "wasm")]
fn take_pending_model_buffers() -> Vec<(usize, usize)> {
    PENDING_MODEL_BUFFERS.with(|b| std::mem::take(&mut *b.borrow_mut()))
}

/// Single-copy load: stream the reader into one wasm-memory buffer, then expose
/// it as a MEMFS file via a zero-copy heap view (`__nbw_wrap_wasm_buffer_as_file`
/// in pre.js), so llama.cpp's `mmap` reads it in place — no JS-heap MEMFS copy.
/// Needs a known `total`. Recorded in `PENDING_MODEL_BUFFERS` for the next
/// `get_model_from_path` to attach to the `Model` (freed on drop).
#[cfg(target_family = "wasm")]
async fn stream_reader_to_wasm_buffer(
    reader: &web_sys::ReadableStreamDefaultReader,
    memfs_path: &str,
    total: usize,
    kind: &str,
    on_progress: Option<&js_sys::Function>,
) -> Result<(), String> {
    // 64-byte aligned so GGUF's 32-byte tensor alignment survives the
    // zero-copy mmap (the mmap base is this buffer's base).
    let layout = std::alloc::Layout::from_size_align(total.max(1), 64)
        .map_err(|e| format!("bad model-buffer layout: {e}"))?;
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        return Err(format!("failed to allocate {total} bytes for model buffer"));
    }

    let mut offset: usize = 0;
    loop {
        let read_result = wasm_bindgen_futures::JsFuture::from(reader.read())
            .await
            .map_err(|e| format!("reader.read(): {e:?}"))?;
        let done = js_sys::Reflect::get(&read_result, &"done".into())
            .ok()
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        if done {
            break;
        }
        let value = js_sys::Reflect::get(&read_result, &"value".into())
            .map_err(|_| "read result missing 'value'".to_string())?;
        let chunk: js_sys::Uint8Array = value
            .dyn_into()
            .map_err(|_| "read chunk is not a Uint8Array".to_string())?;
        let chunk_len = chunk.byte_length() as usize;
        if offset + chunk_len > total {
            return Err(format!(
                "stream exceeded content-length: {offset}+{chunk_len} > {total}"
            ));
        }
        // The single copy: JS chunk bytes -> the wasm-linear buffer.
        unsafe {
            let dst = std::slice::from_raw_parts_mut(base.add(offset), chunk_len);
            chunk.copy_to(dst);
        }
        offset += chunk_len;
        if let Some(cb) = on_progress {
            let _ = cb.call3(
                &JsValue::null(),
                &JsValue::from_f64(offset as f64),
                &JsValue::from_f64(total as f64),
                &JsValue::from_str(kind),
            );
        }
    }
    if offset != total {
        return Err(format!("stream short: got {offset}, expected {total}"));
    }

    // Expose the buffer as a MEMFS file (zero-copy) via the pre.js helper.
    let global = js_sys::global();
    let helper = js_sys::Reflect::get(&global, &"__nbw_wrap_wasm_buffer_as_file".into())
        .map_err(|_| "wrap helper lookup failed".to_string())?;
    let helper_fn: js_sys::Function = helper
        .dyn_into()
        .map_err(|_| "__nbw_wrap_wasm_buffer_as_file is not a function".to_string())?;
    helper_fn
        .call3(
            &JsValue::NULL,
            &JsValue::from_str(memfs_path),
            &JsValue::from_f64(base as usize as f64),
            &JsValue::from_f64(total as f64),
        )
        .map_err(|e| format!("__nbw_wrap_wasm_buffer_as_file failed: {e:?}"))?;

    // Register the buffer for the model to take ownership of (freed on drop).
    PENDING_MODEL_BUFFERS.with(|b| b.borrow_mut().push((base as usize, total)));
    Ok(())
}

/// Fetch a URL into a MEMFS file, caching via the Cache API:
/// - hit: stream the cached response into MEMFS.
/// - miss: `fetch()` → `body.tee()` → one reader streams to MEMFS, the other is
///   cached (in parallel, so the first download isn't slowed).
/// Falls back to an uncached fetch if the Cache API is unavailable.
#[cfg(target_family = "wasm")]
async fn stream_url_to_memfs(
    kind: &str,
    url: &str,
    on_progress: Option<&js_sys::Function>,
) -> Result<std::path::PathBuf, String> {
    let memfs_path = format!("/home/web_user/nbw-{kind}.gguf");

    // --- Cache hit path ---
    if let Some(cache) = open_model_cache().await {
        let matched = wasm_bindgen_futures::JsFuture::from(cache.match_with_str(url))
            .await
            .ok();
        if let Some(ref val) = matched {
            if !val.is_undefined() {
                let response: web_sys::Response = val
                    .clone()
                    .dyn_into()
                    .map_err(|_| "cache hit returned non-Response".to_string())?;
                let total: f64 = response
                    .headers()
                    .get("content-length")
                    .ok()
                    .flatten()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let body = response
                    .body()
                    .ok_or_else(|| "cached response.body is null".to_string())?;
                let reader: web_sys::ReadableStreamDefaultReader = body
                    .get_reader()
                    .dyn_into()
                    .map_err(|_| "expected ReadableStreamDefaultReader".to_string())?;
                if total > 0.0 {
                    stream_reader_to_wasm_buffer(
                        &reader,
                        &memfs_path,
                        total as usize,
                        kind,
                        on_progress,
                    )
                    .await?;
                } else {
                    stream_reader_to_memfs(&reader, &memfs_path, total, kind, on_progress).await?;
                }
                return Ok(std::path::PathBuf::from(memfs_path));
            }
        }
    }

    // --- Cache miss: fetch + tee ---
    let response_jsval = wasm_bindgen_futures::JsFuture::from(fetch_from_global(url))
        .await
        .map_err(|e| format!("fetch failed: {e:?}"))?;
    let response: web_sys::Response = response_jsval
        .dyn_into()
        .map_err(|_| "fetch did not return a Response".to_string())?;
    if !response.ok() {
        return Err(format!(
            "fetch {url}: HTTP {} {}",
            response.status(),
            response.status_text()
        ));
    }
    let total: f64 = response
        .headers()
        .get("content-length")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0);

    let body = response
        .body()
        .ok_or_else(|| "response.body is null".to_string())?;

    // Tee the body: one stream goes to MEMFS, the other to Cache API.
    let teed = body.tee();
    let memfs_stream: web_sys::ReadableStream = js_sys::Reflect::get(&teed, &0.into())
        .map_err(|_| "tee()[0] failed".to_string())?
        .dyn_into()
        .map_err(|_| "tee()[0] not a ReadableStream".to_string())?;
    let cache_stream: web_sys::ReadableStream = js_sys::Reflect::get(&teed, &1.into())
        .map_err(|_| "tee()[1] failed".to_string())?
        .dyn_into()
        .map_err(|_| "tee()[1] not a ReadableStream".to_string())?;

    let reader: web_sys::ReadableStreamDefaultReader = memfs_stream
        .get_reader()
        .dyn_into()
        .map_err(|_| "expected ReadableStreamDefaultReader".to_string())?;

    // Start cache population in the background (best-effort).
    let cache_url = url.to_string();
    let cache_total = total;
    wasm_bindgen_futures::spawn_local(async move {
        if let Some(cache) = open_model_cache().await {
            // Stamp content-length onto the cached response. A Response built
            // from a stream has none, so without this a later cache HIT reads
            // total=0 and falls back to the MEMFS (2-copy) path; with it, the
            // warm load also takes the single-copy wasm-buffer path.
            let init = web_sys::ResponseInit::new();
            if cache_total > 0.0 {
                if let Ok(headers) = web_sys::Headers::new() {
                    let _ = headers.set("content-length", &(cache_total as u64).to_string());
                    init.set_headers(headers.as_ref());
                }
            }
            let cache_resp = web_sys::Response::new_with_opt_readable_stream_and_init(
                Some(&cache_stream),
                &init,
            );
            if let Ok(resp) = cache_resp {
                let _ = wasm_bindgen_futures::JsFuture::from(cache.put_with_str(&cache_url, &resp))
                    .await;
            }
        }
    });

    if total > 0.0 {
        stream_reader_to_wasm_buffer(&reader, &memfs_path, total as usize, kind, on_progress)
            .await?;
    } else {
        stream_reader_to_memfs(&reader, &memfs_path, total, kind, on_progress).await?;
    }

    Ok(std::path::PathBuf::from(memfs_path))
}

/// Convert a `JsValue` (a bare string, or an array of strings and tagged
/// media-part objects) into a core `Prompt`.
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
            // Media rides as bytes, not a MEMFS path: the inference pthread
            // can't read the main thread's MEMFS. Decoded in-memory via
            // `MtmdBitmap::from_buffer` (mtmd auto-detects image vs audio).
            Some("image") | Some("audio") => {
                let bytes = read_media_bytes(&part)?;
                prompt.push_media_bytes(bytes);
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

// ---------- Tool (LLM-callable JS function) ----------
//
// `Tool.fromFn(name, description, jsonSchema, callback)`, passed via
// `Chat.create({ tools: [...] })`. Callbacks are sync or async; the worker↔main
// RPC bridge dispatches each call to the main thread and resumes inference.
// See `js/tests/test_tool.mjs`.

/// Factory namespace for LLM-callable tools (via [`Tool::from_fn`], passed to
/// `Chat`'s `tools` option).
///
/// Tools are plain tagged objects `{__nbwKind: 'tool', name, description,
/// jsonSchema, callback}`, not wasm-bindgen class instances: Rust-defined
/// structs don't `impl JsCast`, so they can't be `dyn_into`'d back out of a
/// generic options object. Tagged objects allow a brand check instead.
#[wasm_bindgen]
pub struct Tool;

#[wasm_bindgen]
impl Tool {
    /// Wrap a JS function as an LLM-callable tool: `name` (identifier),
    /// `description` (when to call), `jsonSchema` (constrains args via the
    /// grammar sampler), `callback` (sync/async; gets parsed args, returns a
    /// string — non-strings are JSON.stringify'd). Returns a tagged
    /// `{__nbwKind:'tool', …}` object the `Chat` constructor unpacks.
    #[wasm_bindgen(js_name = fromFn)]
    pub fn from_fn(
        name: String,
        description: String,
        json_schema: JsValue,
        callback: js_sys::Function,
    ) -> Result<JsValue, JsError> {
        // Validate the schema up-front so a typo errors here, not mid-inference
        // (the Rust side re-parses later; this is just a fast-fail).
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

// ---------- Tool-call proxy (pthread → main thread) ----------
//
// A tool's JS callback is only valid on the main thread, but inference runs on
// a pthread. So the core `Tool` closure sends a `ToolRequest` (name + args +
// reply channel, all `Send`) to a main-thread dispatcher that runs the real
// callback and replies; the pthread closure blocks on the reply.

/// A tool invocation proxied from the inference pthread to the main
/// thread: `(tool_name, args, reply_channel)`.
#[cfg(target_family = "wasm")]
type ToolRequest = (String, serde_json::Value, std::sync::mpsc::Sender<String>);

/// Validate a `Tool.fromFn(...)` tagged object and split it into its
/// parts: `(name, description, schema, callback)`. Runs on the main
/// thread (the callback stays here).
#[cfg(target_family = "wasm")]
fn parse_tagged_tool(
    part: &JsValue,
) -> Result<(String, String, serde_json::Value, js_sys::Function), String> {
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
    let callback: js_sys::Function = js_sys::Reflect::get(part, &"callback".into())
        .map_err(|_| "missing callback".to_string())?
        .dyn_into::<js_sys::Function>()
        .map_err(|_| "callback is not a function".to_string())?;
    Ok((name, description, schema, callback))
}

/// Build a core `Tool` whose async closure proxies the call to the
/// main-thread dispatcher via `req_tx` (the JS callback can't be invoked
/// from the inference pthread — see the module comment above).
#[cfg(target_family = "wasm")]
fn proxy_tool(
    name: String,
    description: String,
    schema: serde_json::Value,
    req_tx: tokio::sync::mpsc::UnboundedSender<ToolRequest>,
) -> nobodywho::tool_calling::Tool {
    let name_for_closure = name.clone();
    nobodywho::tool_calling::Tool::new(
        name,
        description,
        schema,
        std::sync::Arc::new(move |args: serde_json::Value| {
            // Block the inference pthread until the dispatcher runs the JS tool
            // and replies. Safe: the worker is a pthread, so the main loop keeps
            // ticking and resolves the tool's Promise.
            let (reply_tx, reply_rx) = std::sync::mpsc::channel::<String>();
            if req_tx
                .send((name_for_closure.clone(), args, reply_tx))
                .is_err()
            {
                return "ERROR: tool dispatcher is gone".to_string();
            }
            match reply_rx.recv() {
                Ok(s) => s,
                Err(_) => "ERROR: tool reply channel dropped".to_string(),
            }
        }),
    )
}

/// Invoke a JS tool callback on the main thread with the given args and
/// return its result as a string. Handles both sync (string return) and
/// async (Promise-returning) callbacks.
#[cfg(target_family = "wasm")]
async fn invoke_js_callback(callback: &js_sys::Function, args: serde_json::Value) -> String {
    use serde::Serialize as _;
    let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
    let args_js = match args.serialize(&ser) {
        Ok(v) => v,
        Err(e) => return format!("ERROR: tool arg conversion: {e}"),
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
    js_sys::JSON::stringify(&resolved)
        .ok()
        .and_then(|s| s.as_string())
        .unwrap_or_else(|| "ERROR: tool returned a non-serializable value".to_string())
}

/// The main-thread tool registry: tool name → JS callback. Shared
/// (`Rc<RefCell<…>>`) between `Chat` (which updates it on `setTools` /
/// `reset`) and the dispatcher task (which reads it per call).
#[cfg(target_family = "wasm")]
type ToolRegistry = std::rc::Rc<RefCell<std::collections::HashMap<String, js_sys::Function>>>;

/// Spawn the main-thread dispatcher: receive proxied requests, invoke the
/// registered JS callback, reply. Runs until the channel closes (chat dropped);
/// the `KeepAlive` during an in-flight `ask` keeps the event loop pumping.
#[cfg(target_family = "wasm")]
fn spawn_tool_dispatcher(
    registry: ToolRegistry,
    mut req_rx: tokio::sync::mpsc::UnboundedReceiver<ToolRequest>,
) {
    wasm_bindgen_futures::spawn_local(async move {
        while let Some((name, args, reply_tx)) = req_rx.recv().await {
            let cb = registry.borrow().get(&name).cloned();
            let result = match cb {
                Some(cb) => invoke_js_callback(&cb, args).await,
                None => format!("ERROR: no callback registered for tool {name:?}"),
            };
            let _ = reply_tx.send(result);
        }
    });
}

/// Parse a `tools` JS array into core proxy `Tool`s, populating
/// `registry` with the JS callbacks. Each proxy tool routes through
/// `req_tx` to the main-thread dispatcher.
#[cfg(target_family = "wasm")]
fn build_proxy_tools(
    tools_val: &JsValue,
    registry: &ToolRegistry,
    req_tx: &tokio::sync::mpsc::UnboundedSender<ToolRequest>,
) -> Result<Vec<nobodywho::tool_calling::Tool>, JsError> {
    if tools_val.is_undefined() || tools_val.is_null() {
        return Ok(Vec::new());
    }
    let arr = tools_val.dyn_ref::<js_sys::Array>().ok_or_else(|| {
        JsError::new("Chat options.tools must be an array of Tool.fromFn(...) values")
    })?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for i in 0..arr.length() {
        let (name, description, schema, callback) = parse_tagged_tool(&arr.get(i))
            .map_err(|e| JsError::new(&format!("Chat options.tools[{i}]: {e}")))?;
        registry.borrow_mut().insert(name.clone(), callback);
        out.push(proxy_tool(name, description, schema, req_tx.clone()));
    }
    Ok(out)
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
            // Vec<f32> → Float32Array (copies into a fresh wasm typed array).
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
            // Build `[[doc, score], ...]` as nested Arrays (not a serde-converted
            // Vec<(String,f32)>, which the JS side would see as plain Objects).
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

#[cfg(target_family = "wasm")]
const MODEL_CACHE_NAME: &str = "nobodywho-models-v1";

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
    if let Ok(fetch_fn) = js_sys::Reflect::get(&js_sys::global(), &"fetch".into()) {
        if let Some(f) = fetch_fn.dyn_ref::<js_sys::Function>() {
            if let Ok(result) = f.call1(&JsValue::NULL, &JsValue::from_str(url)) {
                if let Ok(promise) = result.dyn_into::<js_sys::Promise>() {
                    return promise;
                }
            }
        }
    }
    js_sys::Promise::reject(&JsValue::from_str(
        "fetch() not available in this global context",
    ))
}

// ---------- TokenStream ----------

/// Token stream returned from `Chat::ask`. Wraps the core `TokenStreamAsync`.
#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub struct TokenStream {
    inner: std::sync::Arc<tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>>,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl TokenStream {
    pub fn next(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let mut stream = inner.lock().await;
            match stream.next_token().await {
                Ok(Some(tok)) => {
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &"value".into(), &JsValue::from_str(&tok));
                    let _ = js_sys::Reflect::set(&obj, &"done".into(), &JsValue::from_bool(false));
                    Ok(obj.into())
                }
                Ok(None) => {
                    let obj = js_sys::Object::new();
                    let _ = js_sys::Reflect::set(&obj, &"value".into(), &JsValue::UNDEFINED);
                    let _ = js_sys::Reflect::set(&obj, &"done".into(), &JsValue::from_bool(true));
                    Ok(obj.into())
                }
                Err(e) => Err::<JsValue, _>(JsError::new(&nobodywho::render_miette(&e))),
            }
        })
    }

    pub fn completed(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let mut stream = inner.lock().await;
            let text = stream
                .completed()
                .await
                .map_err(|e| JsError::new(&nobodywho::render_miette(&e)))?;
            Ok::<JsValue, JsError>(JsValue::from_str(&text))
        })
    }
}

// ---------- Chat ----------

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
pub struct Chat {
    inner: nobodywho::chat::ChatHandleAsync,
    /// Sender for proxying tool calls from the inference pthread to the
    /// main-thread dispatcher. Cloned into each proxy `Tool`.
    tool_req_tx: tokio::sync::mpsc::UnboundedSender<ToolRequest>,
    /// Main-thread tool registry (name → JS callback), shared with the
    /// dispatcher. Updated by `setTools` / `reset`.
    tool_registry: ToolRegistry,
}

#[cfg(target_family = "wasm")]
#[wasm_bindgen]
impl Chat {
    #[wasm_bindgen(js_name = create)]
    pub fn create(opts: JsValue) -> js_sys::Promise {
        promisify(async move {
            let obj = opts
                .dyn_ref::<js_sys::Object>()
                .ok_or_else(|| JsError::new("Chat.create requires an options object"))?;

            let model_url = js_sys::Reflect::get(obj, &"modelUrl".into())
                .ok()
                .and_then(|v| v.as_string());
            let model_path = js_sys::Reflect::get(obj, &"modelPath".into())
                .ok()
                .and_then(|v| v.as_string());
            let mmproj_url = js_sys::Reflect::get(obj, &"mmprojUrl".into())
                .ok()
                .and_then(|v| v.as_string());
            let mmproj_path = js_sys::Reflect::get(obj, &"mmprojPath".into())
                .ok()
                .and_then(|v| v.as_string());
            let system_prompt = js_sys::Reflect::get(obj, &"systemPrompt".into())
                .ok()
                .and_then(|v| v.as_string());
            let context_size = js_sys::Reflect::get(obj, &"contextSize".into())
                .ok()
                .and_then(|v| v.as_f64())
                .map(|v| v as u32);
            // Optional JS progress callback — see Model.load for the contract:
            // onProgress(loaded, total, kind), URL/streaming loads only.
            let on_progress = js_sys::Reflect::get(obj, &"onProgress".into())
                .ok()
                .and_then(|v| v.dyn_into::<js_sys::Function>().ok());

            let model_memfs: std::path::PathBuf = if let Some(path) = model_path {
                mount_host_path_via_nodefs("model", &path).map_err(|e| JsError::new(&e))?
            } else if let Some(url) = model_url {
                stream_url_to_memfs("model", &url, on_progress.as_ref())
                    .await
                    .map_err(|e| JsError::new(&e))?
            } else {
                return Err(JsError::new("Chat.create: pass modelUrl or modelPath"));
            };

            let mmproj_memfs: Option<std::path::PathBuf> = if let Some(path) = mmproj_path {
                Some(mount_host_path_via_nodefs("mmproj", &path).map_err(|e| JsError::new(&e))?)
            } else if let Some(url) = mmproj_url {
                Some(
                    stream_url_to_memfs("mmproj", &url, on_progress.as_ref())
                        .await
                        .map_err(|e| JsError::new(&e))?,
                )
            } else {
                None
            };

            let mut model =
                nobodywho::llm::get_model_from_path(&model_memfs, mmproj_memfs.as_deref(), 0)
                    .map_err(|e| JsError::new(&nobodywho::render_miette(&e)))?;
            // Hand the model its mmap backing buffer(s) so they free on drop.
            for (ptr, size) in take_pending_model_buffers() {
                model.attach_backing_buffer(ptr, size);
            }
            let model = Arc::new(model);

            // Tool-call proxy: JS callbacks stay on the main thread in `registry`;
            // the inference pthread reaches them via `req_tx` → the dispatcher.
            let (req_tx, req_rx) = tokio::sync::mpsc::unbounded_channel::<ToolRequest>();
            let registry: ToolRegistry =
                std::rc::Rc::new(RefCell::new(std::collections::HashMap::new()));
            let tools_jsval = js_sys::Reflect::get(obj, &"tools".into())
                .ok()
                .filter(|v| !v.is_undefined() && !v.is_null());
            let tools = if let Some(tools_val) = tools_jsval {
                build_proxy_tools(&tools_val, &registry, &req_tx)?
            } else {
                Vec::new()
            };
            spawn_tool_dispatcher(registry.clone(), req_rx);

            let mut builder = nobodywho::chat::ChatBuilder::new(model);
            if let Some(ctx) = context_size {
                builder = builder.with_context_size(ctx);
            }
            if let Some(sys) = system_prompt {
                builder = builder.with_system_prompt(Some(sys));
            }
            if !tools.is_empty() {
                builder = builder.with_tools(tools);
            }

            let sampler_val = js_sys::Reflect::get(obj, &"sampler".into())
                .ok()
                .filter(|v| !v.is_undefined() && !v.is_null());
            if let Some(sv) = sampler_val {
                let to_json = js_sys::Reflect::get(&sv, &"toJSON".into())
                    .ok()
                    .and_then(|v| v.dyn_into::<js_sys::Function>().ok())
                    .ok_or_else(|| JsError::new("sampler must be a SamplerConfig"))?;
                let json_str: String = to_json
                    .call0(&sv)
                    .map_err(|e| JsError::new(&format!("sampler.toJSON() failed: {e:?}")))?
                    .as_string()
                    .ok_or_else(|| JsError::new("sampler.toJSON() not a string"))?;
                let cfg: nobodywho::sampler_config::SamplerConfig = serde_json::from_str(&json_str)
                    .map_err(|e| JsError::new(&format!("sampler parse: {e}")))?;
                builder = builder.with_sampler(cfg);
            }

            let template_vars = js_sys::Reflect::get(obj, &"templateVariables".into())
                .ok()
                .filter(|v| !v.is_undefined() && !v.is_null());
            if let Some(tv) = template_vars {
                let vars: std::collections::HashMap<String, bool> =
                    serde_wasm_bindgen::from_value(tv)
                        .map_err(|e| JsError::new(&format!("templateVariables: {e}")))?;
                builder = builder.with_template_variables(vars);
            }

            let handle = builder
                .build_async()
                .map_err(|e| JsError::new(&nobodywho::render_miette(&e)))?;

            Ok(Chat {
                inner: handle,
                tool_req_tx: req_tx,
                tool_registry: registry,
            })
        })
    }

    pub fn ask(&self, prompt: JsValue) -> Result<TokenStream, JsError> {
        let p = js_to_prompt(&prompt).map_err(|e| JsError::new(&e))?;
        let stream = self.inner.ask(p);
        Ok(TokenStream {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(stream)),
        })
    }

    #[wasm_bindgen(js_name = stopGeneration)]
    pub fn stop_generation(&self) {
        self.inner.stop_generation();
    }

    #[wasm_bindgen(js_name = getChatHistory)]
    pub fn get_chat_history(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let messages = inner
                .get_chat_history()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            serde_wasm_bindgen::to_value(&messages).map_err(|e| JsError::new(&e.to_string()))
        })
    }

    #[wasm_bindgen(js_name = setChatHistory)]
    pub fn set_chat_history(&self, messages: JsValue) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let msgs: Vec<nobodywho::chat::Message> = serde_wasm_bindgen::from_value(messages)
                .map_err(|e| JsError::new(&format!("messages: {e}")))?;
            inner
                .set_chat_history(msgs)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = getSystemPrompt)]
    pub fn get_system_prompt(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let prompt = inner
                .get_system_prompt()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(match prompt {
                Some(s) => JsValue::from_str(&s),
                None => JsValue::NULL,
            })
        })
    }

    #[wasm_bindgen(js_name = setSystemPrompt)]
    pub fn set_system_prompt(&self, prompt: JsValue) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let p = if prompt.is_null() || prompt.is_undefined() {
                None
            } else {
                prompt.as_string()
            };
            inner
                .set_system_prompt(p)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = getSamplerConfig)]
    pub fn get_sampler_config(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let cfg = inner
                .get_sampler_config()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(SamplerConfig { inner: cfg })
        })
    }

    #[wasm_bindgen(js_name = setSamplerConfig)]
    pub fn set_sampler_config(&self, sampler: &SamplerConfig) -> js_sys::Promise {
        let cfg = sampler.inner.clone();
        let inner = self.inner.clone();
        promisify(async move {
            inner
                .set_sampler_config(cfg)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = getTemplateVariables)]
    pub fn get_template_variables(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let vars = inner
                .get_template_variables()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            use serde::Serialize;
            let ser = serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true);
            vars.serialize(&ser)
                .map_err(|e| JsError::new(&e.to_string()))
        })
    }

    #[wasm_bindgen(js_name = setTemplateVariable)]
    pub fn set_template_variable(&self, name: String, value: bool) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            inner
                .set_template_variable(name, value)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = setTemplateVariables)]
    pub fn set_template_variables(&self, variables: JsValue) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            let vars: std::collections::HashMap<String, bool> =
                serde_wasm_bindgen::from_value(variables)
                    .map_err(|e| JsError::new(&format!("variables: {e}")))?;
            inner
                .set_template_variables(vars)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = setTools)]
    pub fn set_tools(&self, tools: JsValue) -> js_sys::Promise {
        // Rebuild the registry + proxy tools on the main thread (JS callbacks
        // live here), then hand the proxy tools to the inference worker.
        self.tool_registry.borrow_mut().clear();
        let t = match build_proxy_tools(&tools, &self.tool_registry, &self.tool_req_tx) {
            Ok(t) => t,
            Err(e) => return js_sys::Promise::reject(&e.into()),
        };
        let inner = self.inner.clone();
        promisify(async move {
            inner
                .set_tools(t)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = reset)]
    pub fn reset(&self, opts: JsValue) -> js_sys::Promise {
        let (prompt, tools_val) = if opts.is_undefined() || opts.is_null() {
            (None, JsValue::UNDEFINED)
        } else {
            let p = js_sys::Reflect::get(&opts, &"systemPrompt".into()).unwrap_or(JsValue::NULL);
            let t = js_sys::Reflect::get(&opts, &"tools".into()).unwrap_or(JsValue::UNDEFINED);
            (
                if p.is_null() || p.is_undefined() {
                    None
                } else {
                    p.as_string()
                },
                t,
            )
        };
        // Rebuild the registry + proxy tools synchronously (main thread).
        self.tool_registry.borrow_mut().clear();
        let tools = match build_proxy_tools(&tools_val, &self.tool_registry, &self.tool_req_tx) {
            Ok(t) => t,
            Err(e) => return js_sys::Promise::reject(&e.into()),
        };
        let inner = self.inner.clone();
        promisify(async move {
            inner
                .reset_chat(prompt, tools)
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }

    #[wasm_bindgen(js_name = resetHistory)]
    pub fn reset_history(&self) -> js_sys::Promise {
        let inner = self.inner.clone();
        promisify(async move {
            inner
                .reset_history()
                .await
                .map_err(|e| JsError::new(&e.to_string()))?;
            Ok(JsValue::UNDEFINED)
        })
    }
}
