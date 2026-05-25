# nobodywho-js

WebAssembly binding for [NobodyWho](https://nobodywho.ooo) — runs local LLMs
in a browser tab (or any wasm host) via llama.cpp compiled to wasm32 with
Emscripten.

## Status: working end-to-end

Real LLM inference verified under Node. The demos mirror the Python
examples next door — load a model, call the API:

**Chat** (`chat_demo.mjs`):
```js
import { readFileSync } from 'node:fs';
const { default: createNobodyWhoModule } = await import('./pkg-bundler/nobodywho_js.js');
const m = await createNobodyWhoModule();
m.init();

const modelBytes = new Uint8Array(readFileSync(process.argv[2]));
const chat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'You are a helpful assistant',
});

const result = await chat.ask('What is the capital of Denmark?').completed();
console.log(result);
```

`Chat` runs inference in a separate worker thread — a Web Worker in
browsers, a `worker_threads.Worker` in Node — so the calling thread
stays responsive during inference. Tokens stream in real time: the
worker `postMessage`s each sampled token to main as it's generated.

**Streaming** — to consume tokens incrementally, async-iterate the
`TokenStream` and update the UI per token:

```js
for await (const tok of chat.ask('Write a short paragraph about Copenhagen.')) {
  process.stdout.write(tok);   // or: outputEl.textContent += tok;
}
```

Equivalent explicit form (same result, useful if you want loop-level
control):

```js
const stream = chat.ask(prompt);
while (true) {
  const { value, done } = await stream.next();
  if (done) break;
  process.stdout.write(value);
}
```

**Embedding** (`encoder_demo.mjs`):
```js
const m = await createNobodyWhoModule();
m.init();

const model = await m.Model.loadBytes(modelBytes);
const encoder = new m.Encoder(model, 2048);
const vec = await encoder.encode('the quick brown fox');
// -> Float32Array(384) — pass to cosineSimilarity() etc.
```

The wasm binary contains all of llama.cpp and exposes the binding's full
surface to JS via wasm-bindgen.

| Surface | Status |
|---|---|
| `Model.loadBytes(uint8Array)` | ✅ verified — loads GGUF into a real `LlamaModel` via `fmemopen` + `llama_model_load_from_file_ptr` (used by `Encoder` / `CrossEncoder`; `Chat.create` takes `modelBytes` inline) |
| `Encoder.encode(text)` → `Float32Array` | ✅ verified |
| `CrossEncoder.rank(query, docs)` / `rankAndSort(...)` | ✅ verified |
| `cosineSimilarity(a, b)` → number | ✅ verified — pairs with `Encoder.encode()`; matches Python's `nobodywho.cosine_similarity` |
| `Chat.create({modelBytes \| modelPath \| modelUrl, ...})` → `Chat` | ✅ verified — async factory, spawns a worker (Web Worker in browser, `worker_threads.Worker` in Node). `modelPath` is Node-only and streams the GGUF into MEMFS in chunks (no 2 GiB `readFileSync` cap, no main-thread Buffer) |
| `Chat.ask(prompt)` → `TokenStream` → tokens (real-time, `for await`-iterable) | ✅ verified |
| `Chat.askStreaming(prompt, callback)` → `Promise<string>` | ✅ verified — fires `callback(token)` per token, resolves to the full response. Sugar over the for-await pattern; mirrors the older binding's API name for migration ease |
| `Chat.stopGeneration()` — interrupt the current ask | ✅ verified (Node) — drains pending messages in the per-token hook and calls `stopCurrentAsk`. Browser only takes effect after the current ask completes (SAB+Atomics is the path forward, tracked as follow-up) |
| `Chat.getChatHistory()` / `setChatHistory(messages)` | ✅ verified — round-trips `{role, content, ...}` arrays; loaded history is real context for subsequent asks |
| `Chat.getSystemPrompt()` / `setSystemPrompt(prompt \| null)` | ✅ verified (incl. clear-to-null after the core fix in `chat.rs`) |
| `Chat.getSamplerConfig()` / `setSamplerConfig(spec)` | ✅ verified |
| `Chat.getTemplateVariables()` / `setTemplateVariable(name, value)` / `setTemplateVariables(vars)` | ✅ verified |
| `Chat.setTools(tools)` — replace tool registry mid-session | ✅ verified |
| `Chat.reset(opts?)` — atomic clear-history + optional swap of system prompt + tools | ✅ verified |
| `Chat.resetHistory()` — clear history, preserve system prompt + tools + sampler | ✅ verified |
| `Chat.terminate()` — shut down the worker (returns Promise) | ✅ verified |
| `Chat`'s `templateVariables` option (e.g. `{ enable_thinking: false }`) | ✅ verified |
| `Chat`'s `sampler` option (temperature/topK/topP/minP/repeatPenalty/sampleStep) | ✅ verified |
| `SamplerBuilder` (fluent) / `SamplerPresets` (static factory) | ✅ verified — JS-side ergonomic wrappers over the sampler JSON shape; matches Python's `SamplerBuilder` / `SamplerPresets` |
| `TokenStream.next()` / `completed()` / async-iteration via `for await` | ✅ verified |
| Multimodal vision/audio (`Image.fromBytes` / `Audio.fromBytes`, plus Node-only `fromPath`) | ✅ verified — see "Multimodal status" below |
| Tool calling (`Tool.fromFn(...)`, `Chat.create({tools: [...]})`) | ✅ verified — both sync and async (Promise-returning) callbacks work via the worker ↔ main RPC bridge |
| Structured output (`Chat.create({constraint: {regex \| jsonSchema \| lark}})`) | ✅ verified — see `js/scripts/constraint-smoke.mjs` (regex + JSON Schema) |

Each row above is backed by a smoke test under `js/scripts/`. To
verify locally after a build, run:

| Smoke | Covers |
|---|---|
| `emscripten-smoke.mjs` | `Model.loadBytes` + basic ask round-trip (plus the `chat_demo.mjs` example end-to-end) |
| `forawait-smoke.mjs` | `for await (const tok of chat.ask(...))` iteration |
| `askstreaming-smoke.mjs` | `Chat.askStreaming(prompt, callback)` per-token callback shape |
| `sampler-smoke.mjs` | sampler-config knobs end-to-end |
| `sampler-ergo-smoke.mjs` | `SamplerBuilder` + `SamplerPresets` (core shift + sample steps) |
| `sampler-extra-smoke.mjs` | DRY / XTC / TypicalP / full Penalties shift steps + `dry()` / `json()` presets |
| `constraint-smoke.mjs` | structured output (regex / JSON Schema / lark) |
| `tool-smoke.mjs` | sync + async tool callbacks via the worker RPC bridge |
| `audio-smoke.mjs` | WAV / MP3 / FLAC decoder paths end-to-end |
| `vision-smoke.mjs` | image input through mtmd (Qwen2-VL / Gemma 3 etc.) |
| `stop-smoke.mjs` | `Chat.stopGeneration()` interrupting an in-flight ask |
| `terminate-mid-stream-smoke.mjs` | `Chat.terminate()` cleanly fails an in-flight TokenStream |
| `history-smoke.mjs` | `getChatHistory` / `setChatHistory` round-trip + loaded-context use |
| `setters-smoke.mjs` | `setSystemPrompt` (incl. `null`) / sampler / template vars / `setTools` / `resetHistory` |
| `parity-extras-smoke.mjs` | `Audio.fromPath` / `Image.fromPath` / `cosineSimilarity` / `Chat.reset({systemPrompt, tools})` |
| `modelpath-smoke.mjs` | Node-only `Chat.create({modelPath, mmprojPath})` |
| `context-shift-smoke.mjs` | KV-cache shift when the conversation grows past `contextSize` |

Each smoke prints a `=== passed ===` line on success and exits 0; CI
runs them in sequence.

Native (`cargo check --workspace`) is unchanged.

## Build pipeline

```
   nobodywho/core (Rust)
        +
   llama-cpp-2 fork @ nobodywho-ooo/llama-cpp-rs branch wasm-emscripten
        |
        | emcc (Emscripten C/C++ toolchain) for the llama.cpp side
        | rustc + wasm-bindgen attrs for the Rust side
        v
   wasm32-unknown-emscripten .wasm
        |
        | patched wasm-bindgen-cli (nobodywho-ooo/wasm-bindgen fork)
        | + post-link emcc invocation with --js-library
        v
   pkg-bundler/
     ├── nobodywho_js.js          (Emscripten loader factory)
     ├── nobodywho_js_bg.wasm     (linked wasm)
     ├── nobodywho_js.wasm        (mirrored copy some consumers expect)
     ├── library_bindgen.js       (kept for debugging)
     └── pre.js                   (HEAP_DATA_VIEW shim, inlined by emcc)
```

## Build it yourself

### Prerequisites

You need a patched Emscripten fork plus a patched wasm-bindgen fork. Both
are intermediate — the patches are filed upstream and we'll drop the forks
when they land.

```bash
# 1. Emscripten with the -sWASM_BINDGEN setting
#    (PR emscripten-core/emscripten#23493)
git clone https://github.com/walkingeyerobot/emscripten ~/emscripten-wbg
cd ~/emscripten-wbg
./bootstrap   # downloads the matching binaryen + node bundle

# 2. wasm-bindgen with descriptor-interpreter + Emscripten-output-mode fixes
git clone https://github.com/nobodywho-ooo/wasm-bindgen ~/wasm-bindgen
cargo install --path ~/wasm-bindgen/crates/cli \
  --root /tmp/wbg-patched --locked

# 3. rustc target
rustup target add wasm32-unknown-emscripten
```

### Build

```bash
bash nobodywho/js/scripts/build-pkg-emscripten.sh
```

This invokes cargo → injects the `__wasm_bindgen_emscripten_marker` custom
section into the linked wasm → runs the patched wasm-bindgen-cli to emit
`library_bindgen.js` → applies a handful of sed patches for codegen
quirks → runs emcc again in `--post-link` mode with the resulting JS
library, producing the runnable bundle in `pkg-bundler/`.

The `pkg-bundler/` directory is the npm-publishable artifact (minus a
hand-written `package.json` — see `js/package.json.tpl`).

## Run it

### Under Node

```bash
node js/examples/encoder_demo.mjs /path/to/embedding-model.gguf
node js/examples/chat_demo.mjs    /path/to/chat-model.gguf
node js/examples/crossencoder_demo.mjs /path/to/crossencoder-model.gguf
```

Node 20+ should work; 22+ is verified.

### In a browser

```bash
cd nobodywho/js
python3 -m http.server 8000
# open http://localhost:8000/examples/browser-chat.html
```

See `js/examples/browser-chat.html` for a chat demo (Web Worker so the
page stays responsive during inference), `browser-encoder.html` for an
embeddings demo, and `browser-crossencoder.html` for a reranker demo.
All load the wasm via `createNobodyWhoModule()` and fetch a GGUF from
HuggingFace through `Model.fetchModelBytes(url, onProgress)` (exposed
from the Rust side). Model bytes are cached in the Cache API
(`nobodywho-models-v1` store) so subsequent loads skip the download.

Note: the native binding has `huggingface:` / `https://` paths that
download and cache models on disk (see Python's `Model("hf://…")`), but
that codepath is `cfg(not(target_family = "wasm"))` — `ureq` has no browser equivalent
and there's no filesystem to cache into. The browser-side equivalent is
just `fetch()` + `Model.loadBytes(...)`, with caching via the Cache API
(see next section).

### Model caching

`Model.fetchModelBytes` caches downloaded GGUFs in a
[Cache API](https://developer.mozilla.org/en-US/docs/Web/API/Cache)
store named `nobodywho-models-v1`, keyed by URL. After the first download
on a given origin, reloads and other pages on the same origin get the
bytes back instantly from disk — no re-download, no HTTP cache eviction
surprises.

Helpers on the `Model` class:

```js
const { default: createNobodyWhoModule } = await import('nobodywho-js');
const m = await createNobodyWhoModule();
m.init();

// Pre-populate the cache during a splash screen / onboarding step so the
// user doesn't sit through a 400 MB download when they click "chat".
await m.Model.preload('https://huggingface.co/.../model.gguf',
  (got, total) => console.log(`${got}/${total}`));

// Later in the app — instant if preload already ran on this origin.
const bytes = await m.Model.fetchModelBytes('https://huggingface.co/.../model.gguf');
const model = await m.Model.loadBytes(bytes);

// Wipe all cached models (e.g. from a "clear cache" button).
await m.Model.clearCache();
```

**Across multiple apps on different origins:** the browser's storage is
[origin-partitioned](https://developer.mozilla.org/en-US/docs/Web/API/Storage_API#storage_quotas_and_eviction_criteria)
by design — `app1.example.com` and `app2.example.com` each have their own
Cache API, and there's no way for one to read the other's entries. The
best you can do cross-origin is point all apps at the same canonical
HuggingFace URL: the HF CDN serves each origin's first download from a
geographically-near edge, so it's fast even though every origin pays the
download once. If you control the deployment, hosting multiple apps under
one origin (subpath routing, e.g. `myco.com/chat`, `myco.com/embed`)
lets them share the cache.

The cache name is versioned (`-v1`). Bump the suffix if the cached
representation ever changes (e.g. switching from raw bytes to a
pre-decoded format) so old entries are abandoned rather than fed to a
binding that can't read them.

## How it works (and why these specific choices)

### Target: `wasm32-unknown-emscripten`

Emscripten provides the C/C++ toolchain (libc, libc++, malloc, an
in-memory filesystem, syscall shims) that llama.cpp's C/C++ side needs.
Compiling Rust to wasm32 directly and linking against Emscripten's
libraries gives us a single wasm that hosts both halves of the binding.

The Rust side uses wasm-bindgen attributes to expose typed JS classes
(`Model`, `Chat`, `Encoder`, `CrossEncoder`, `Image`, `Audio`,
`Tool`, etc.). Stock wasm-bindgen-cli doesn't yet ship Emscripten
output mode, so we use a temporary fork
(`nobodywho-ooo/wasm-bindgen`) until upstream merges those changes.

### Source-level patches to llama.cpp

The fork at [`nobodywho-ooo/llama-cpp-rs` branch
`wasm-emscripten`](https://github.com/nobodywho-ooo/llama-cpp-rs/tree/wasm-emscripten)
carries a small set of build-system tweaks:

- `CMAKE_SYSTEM_PROCESSOR=wasm32` so the SIMD quant kernels select the
  right code path under the Emscripten toolchain.
- `-fexceptions` on the mtmd C++ TUs (multimodal needs C++ exception
  support for `std::ifstream` error paths).
- `MA_NO_DEVICE_IO`, `MA_NO_THREADING`, `MA_NO_ENGINE`,
  `MA_NO_NODE_GRAPH`, `MA_NO_RESOURCE_MANAGER`, `MA_NO_GENERATION`
  defines for miniaudio — removes every pthread-using piece while
  keeping the file-header sniffer and the format decoders.

These will land upstream as opt-in cmake flags once a few rounds of
review settle.

### Runtime workarounds in nobodywho

A few cfg-gates in `nobodywho/core` for wasm32:

- `tokio` features: drop `rt-multi-thread` (no OS threads).
- `ureq`, `indicatif`, `dirs`, `monty`, `bashkit`: native-only.
- Worker pattern: `std::thread::spawn` → `wasm_bindgen_futures::spawn_local`,
  `std::sync::mpsc` → `tokio::sync::mpsc::unbounded_channel`.
- Model loading: `get_model_from_bytes` constructor that bypasses the
  filesystem (`fmemopen` + `llama_model_load_from_file_ptr`).
- `Tokenizer::tokenize_text` inlines the `mtmd_default_marker` literal.
- `Worker` n_threads hardcoded to 1 (`available_parallelism` errors on
  wasm).
- `mtmd` cargo feature on core stays enabled — Emscripten compiles it
  in (see "Multimodal status").

And one workaround in the wasm crate itself (`js/src/lib.rs`):
- `__cxa_atexit` overridden as a no-op. The cdylib's link wraps every
  export in `__wasm_call_ctors` / `__wasm_call_dtors`, and the dtor
  walk trips on a signature mismatch in one of libc++'s static
  destructors. Suppressing the registration entirely makes the dtor
  walk a no-op. Global destructors don't run at module shutdown —
  fine, the wasm instance lives for the JS process anyway.

## Multimodal status

Vision and audio input work end-to-end through bytes — no filesystem,
no path-based APIs surfaced to JS callers. Architecturally the JS
binding virtualizes a filesystem in Emscripten's MEMFS, lands bytes
there, and lets llama.cpp's path-based loaders read them from the
inside. All of upstream mtmd is used unchanged.

**End-to-end validation.** `js/scripts/vision-smoke.mjs` against
Qwen2-VL-2B-Instruct (Q4_K_M model + Q8_0 mmproj, ~1.7 GB total):

```
mmproj_path = Some("/home/web_user/nbw-mmproj-...gguf");
loaded meta data with 33 key-value pairs and 338 tensors from (file*)
load_tensors: loaded 520 tensors from /home/web_user/nbw-mmproj-...gguf
MTMD context initialized successfully
Loading image for MTMD path = /home/web_user/nbw-image-...bin;
image_tokens->nx = 46, ny = 54
=== Response (1099.0 s) ===
penguin
contains "penguin": true
```

The whole chain — `Model.loadBytes(model, mmproj)` → `Image.fromBytes(uint8)`
→ `chat.ask([...])` → "penguin" — verified inside `node`. 1099 s CPU
inference reflects the wasm32 single-threaded ceiling; architecture
is the point of this section, not throughput.

**JS API.**

```js
const { default: createNobodyWhoModule } = await import('nobodywho-js');
const m = await createNobodyWhoModule();
m.init();

const modelBytes  = new Uint8Array(await (await fetch('/model.gguf')).arrayBuffer());
const mmprojBytes = new Uint8Array(await (await fetch('/mmproj.gguf')).arrayBuffer());

// Optional mmproj as a second arg — promotes the Model to multimodal.
const model = await m.Model.loadBytes(modelBytes, mmprojBytes);

const chat = new m.Chat(model, {
  systemPrompt: 'Describe the image.',
  contextSize: 4096,            // ≥ image embedding + reply
});

const imgResp = await fetch('/cat.jpg');
const img = m.Image.fromBytes(new Uint8Array(await imgResp.arrayBuffer()));

const answer = await (await chat.ask(['What is in this image?', img])).completed();
```

`Image.fromBytes(uint8)` / `Audio.fromBytes(uint8)` return plain JS
objects of shape `{__nbwKind: 'image' | 'audio', bytes: Uint8Array}`.
They are structured-cloneable, which is what lets them survive the
postMessage hop into `Chat`'s background worker.

The array-of-parts shape is accepted by `Chat.ask`. Plain strings
still work for text-only prompts — `chat.ask('hi')` is unchanged.

**How it works under the hood** (see `js/src/syscall_imports.rs` and
`js/src/lib.rs` for code, this is just the shape):
1. JS calls `Image.fromBytes(uint8)` → tagged object.
2. `Chat.ask([...])` postMessages the array to the worker.
3. Worker-side Rust receives the bytes, calls `Module.FS.writeFile`
   via `js_sys::Reflect` to land them at a content-hashed path like
   `/home/web_user/nbw-image-<hash>.bin` in Emscripten's MEMFS.
4. The same Rust calls the existing `Prompt::push_image(&Path)`.
5. mtmd's C++ side opens the file via libc `fopen("rb")`. Libc's
   `__syscall_openat` is satisfied by a strong Rust override (also in
   `syscall_imports.rs`) that resolves the call back into
   `Module.FS.open` via `js_sys::Reflect` — completing the loop.

The strong override is necessary because Emscripten's
`system/lib/standalone/standalone.c` provides weak `__syscall_openat`
stubs that always return `-EPERM`. wasm-ld silently satisfies libc's
syscall references against the weak stubs unless a strong symbol is
present. The Rust override wins symbol resolution at link time.

**What's known to work.**

- Image decoding via `stb_image`: JPEG, PNG, BMP, GIF, TGA, PSD, PIC,
  PNM. Format is sniffed from the file header by mtmd.
- Three miniaudio decoders end-to-end: **WAV, MP3, FLAC** — verified
  by `js/scripts/audio-smoke.mjs` (Qwen3-ASR produces real
  transcripts).
- mmproj loading from bytes via `Model.loadBytes(model, mmproj)`.
- Vision encoder via mtmd (chunk-tokenize / encode-chunk / decode
  pipeline, verified against Gemma 3 + Qwen2-VL mmprojs).
- Audio-LLM mmproj encoder via mtmd (Qwen3-ASR verified end-to-end
  for WAV/MP3/FLAC — the model produces real transcripts). Requires
  our patched `mtmd-audio.cpp` (cfg-gates the mel preprocessor's
  `n_threads` to 1 on Emscripten); upstream's hardcoded `n_threads=4`
  traps on `pthread_create` in a wasm build without pthread.

**What's known not to work / untested.**

- Audio playback / device IO. `MA_NO_DEVICE_IO` removes the
  `ma_context` / `ma_device` machinery (pthread-owned). The wasm has
  no `AudioContext` / `WebAudio` bridge — models that *generate*
  audio (TTS) would need to post their PCM samples to the page for
  Web Audio playback.
- Chat templates that use OpenAI-style typed-content arrays (SmolVLM,
  some Phi-3-Vision variants). Our `core/src/template.rs` emits the
  older string-with-markers shape (`<__media__>` placeholder). Qwen,
  Gemma, LLaVA, etc. use the string-with-markers convention and
  work fine; OpenAI-typed-content models would need a renderer
  update.
- Models >2 GiB on disk via `modelBytes` in Node. Node's
  `fs.readFileSync` caps at 2 GiB. Use `Chat.create({ modelPath })`
  instead — the worker streams the file into MEMFS in 64 MiB chunks
  with no main-thread Buffer of the model bytes, bypassing the cap.
  Browser is unaffected (`modelUrl` streams via fetch + Cache API).
- Models whose total working set exceeds 4 GiB on the wasm32 build.
  Model tensors + mmproj + KV cache + compute buffer must fit in
  wasm32's hard 4 GiB linear-memory ceiling. Gemma 3 4B Q4_K_M +
  mmproj overflows. The sibling wasm64 build pipeline
  (`scripts/build-pkg-emscripten-wasm64.sh`) lifts the ceiling to
  16 GiB, but is currently spike-quality and requires manual
  toolchain setup (nightly Rust + one-line rustlib unwind patch
  pending in [rust-lang/rust#156912][rust-pr156912]).

[rust-pr156912]: https://github.com/rust-lang/rust/pull/156912

## Outstanding

- **Upstream the four forks pinned by this PR.** Each carries
  changes that need to land in their respective upstream projects so we
  can drop the fork pointer. All four are publicly hosted under
  `nobodywho-ooo/` for transparency / reproducibility:
  - [`nobodywho-ooo/llama-cpp-rs` branch `wasm-emscripten`](https://github.com/nobodywho-ooo/llama-cpp-rs/tree/wasm-emscripten)
    — `CMAKE_SYSTEM_PROCESSOR=wasm32` for ggml's wasm SIMD quants, `MA_NO_*`
    defines + `-fexceptions` for mtmd. Pulled directly via `core/Cargo.toml`
    `{ git = "...", branch = "wasm-emscripten" }`. Its `llama.cpp`
    submodule is pinned at our fork (next bullet).
  - [`nobodywho-ooo/llama.cpp` branch `wasm-emscripten`](https://github.com/nobodywho-ooo/llama.cpp/tree/wasm-emscripten)
    — one patch on top of upstream: `5e6a3d945` cfg-gates the
    mtmd-audio mel spectrogram's `n_threads` to 1 on `__EMSCRIPTEN__`
    (avoids the `pthread_create` trap that killed audio inference
    for all formats). Pulled via the llama-cpp-rs submodule.
  - [`nobodywho-ooo/wasm-bindgen` branch `emscripten-descriptor-fixes`](https://github.com/nobodywho-ooo/wasm-bindgen/tree/emscripten-descriptor-fixes)
    — descriptor-interpreter tolerance for Emscripten-shaped wasm.
    Pinned via the workspace `Cargo.toml` `[patch.crates-io]` block.
  - [`nobodywho-ooo/emscripten` branch `wbg-walkingeyerobot`](https://github.com/nobodywho-ooo/emscripten/tree/wbg-walkingeyerobot)
    — fork of `walkingeyerobot/emscripten` (which itself carries the
    `-sWASM_BINDGEN` flag tracked in [emscripten-core/emscripten#23493](https://github.com/emscripten-core/emscripten/pull/23493)).
    Consumed at build time via `$EMSDK_DIR` pointing at a local clone.
