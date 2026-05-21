# nobodywho-js

WebAssembly binding for [NobodyWho](https://nobodywho.ooo) — runs local LLMs
in a browser tab (or any wasm host) via llama.cpp compiled to wasm32.

## Status: working end-to-end

Real LLM inference verified under Node. The demos mirror the Python
examples next door — load a model, call the API:

**Chat** (`chat_demo.mjs`, ~15 lines):
```js
import { readFileSync } from 'node:fs';
import { Model, Chat } from './setup.mjs';

const model = await Model.loadBytes(new Uint8Array(readFileSync(process.argv[2])));
const chat = new Chat(model, { systemPrompt: 'You are a helpful assistant' });

const result = await (await chat.ask('What is the capital of Denmark?')).completed();
console.log(result);
```

**Embedding** (`encoder_demo.mjs`, ~40 lines including cosine similarity):
```js
import { Model, Encoder } from './setup.mjs';

const model = await Model.loadBytes(modelBytes);
const encoder = new Encoder(model, 2048);
const vec = await encoder.encode('the quick brown fox');
// -> Float32Array(384) — pass to cosineSimilarity() etc.
```

`setup.mjs` hides the WASI + wasm-bindgen wiring (~50 lines) so each demo
stays focused on the API. The eventual `nobodywho-js` npm package will
fold that bootstrap into its own entry point, at which point the demos
collapse to `import { Model, Chat } from 'nobodywho-js'`.

The wasm binary contains all of llama.cpp (~9.5 MB release, ~21 MB debug)
and exposes the binding's full surface to JS via wasm-bindgen.

| Surface | Status |
|---|---|
| `Model.loadBytes(uint8Array)` | ✅ verified — loads GGUF into a real `LlamaModel` via `fmemopen` + `llama_model_load_from_file_ptr` |
| `Encoder.encode(text)` → `Float32Array` | ✅ verified |
| `CrossEncoder.rank(query, docs)` / `rankAndSort(...)` | ✅ verified |
| `Chat.ask(prompt)` → `TokenStream` → tokens | ✅ verified |
| `Chat`'s `templateVariables` option (e.g. `{ enable_thinking: false }`) | ✅ verified |
| `TokenStream.nextToken()` / `completed()` | ✅ verified |
| Multimodal vision/audio (`Image.fromBytes` / `Audio.fromBytes`) | ✅ compiles — see "Multimodal status" below for what's actually supported on Emscripten |
| Structured output (`Constraint`) | wire format exposed via `Chat`'s options (see `ConstraintSpec` in `js/src/lib.rs`); should now work on Emscripten (libc has `clock_gettime`), unverified |

Native (`cargo check --workspace`) is unchanged.

## Build pipeline

```
   nobodywho/core (Rust)
        +
   llama-cpp-2 fork @ nobodywho-ooo/llama-cpp-rs branch wasm
        |
        | wasi-sdk clang for the C/C++ side
        | rustc + wasm-bindgen attrs for the Rust side
        v
   wasm32-unknown-unknown .wasm (21 MB debug, 9.5 MB release)
        |
        | wasm-bindgen-cli --target bundler
        v
   pkg-bundler/
     ├── nobodywho_js.js          (entry — calls __wbg_set_wasm)
     ├── nobodywho_js_bg.js       (Chat/Model/Encoder classes + glue)
     ├── nobodywho_js_bg.wasm     (compiled wasm)
     ├── nobodywho_js.d.ts        (TS typings)
     └── nobodywho_js_bg.wasm.d.ts
```

## Build it yourself

### Prerequisites

```bash
# wasi-sdk for compiling the C/C++ side of llama.cpp.
# Builds tested with v33; older versions likely work.
curl -L https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-33/wasi-sdk-33.0-arm64-macos.tar.gz \
  | tar -xz -C ~
export WASI_SDK_PATH=~/wasi-sdk-33.0-arm64-macos
# Linux: replace arm64-macos with x86_64-linux or arm64-linux.

# rustc target + wasm-bindgen-cli.
# The CLI version must match the wasm-bindgen crate in Cargo.lock — the
# helper script reads it from there so there's one source of truth.
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli \
  --version "$(bash js/scripts/wasm-bindgen-version.sh)" \
  --locked
```

### Build

```bash
cd nobodywho

WASI_SDK_PATH=$WASI_SDK_PATH \
  cargo build --target wasm32-unknown-unknown --release -p nobodywho-js

wasm-bindgen --target bundler \
  target/wasm32-unknown-unknown/release/nobodywho_js.wasm \
  --out-dir js/pkg-bundler/
```

The `pkg-bundler/` directory is the npm-publishable artifact (minus a
hand-written `package.json` — see `js/package.json.tpl`).

## Run it

### Under Node (uses `node:wasi` for WASI imports)

The shipped `.wasm` uses wasm-gc and wasm exception-handling value types
(emitted by wasi-sdk's libc++ STL — see `money_get<wchar_t>` and friends).
Node version support:

- **Node 26+** — works out of the box.
- **Node 24-25** — pass `--experimental-wasm-exnref` to `node`.
- **Node 22-23** — V8 has a SIGSEGV on Linux x86_64 (fixed in 24); macOS
  arm64 works with the experimental flag. Not officially supported.
- **Node 20 and older** — not supported; the wasm fails validation
  before any code runs.

The `engines.node` field is set to `>=24` to reflect this.

```bash
# Real embedding inference with a GGUF:
node js/examples/encoder_demo.mjs /path/to/embedding-model.gguf

# Chat (when you have a chat-style GGUF):
node js/examples/chat_demo.mjs /path/to/chat-model.gguf

# Cross-encoder reranking (when you have a reranker GGUF):
node js/examples/crossencoder_demo.mjs /path/to/crossencoder-model.gguf
```

### In a browser (uses `@bjorn3/browser_wasi_shim`)

```bash
cd nobodywho/js
python3 -m http.server 8000
# open http://localhost:8000/examples/browser-chat.html
```

See `js/examples/browser-chat.html` for a chat demo (Web Worker so the
page stays responsive during inference), `browser-encoder.html` for an
embeddings demo, and `browser-crossencoder.html` for a reranker demo.
All load the wasm, polyfill WASI via `@bjorn3/browser_wasi_shim`, and
fetch a GGUF from HuggingFace through `setup-browser.mjs`'s
`fetchModelBytes(url, onProgress)` helper. Model bytes are cached in
the Cache API (`nobodywho-models-v1` store) so subsequent loads skip
the download.

Note: the native binding has `huggingface:` / `https://` paths that
download and cache models on disk (see Python's `Model("hf://…")`), but
that codepath is `cfg(not(wasm32))` — `ureq` has no browser equivalent
and there's no filesystem to cache into. The browser-side equivalent is
just `fetch()` + `Model.loadBytes(...)`, with caching via the Cache API
(see next section).

### Model caching

`fetchModelBytes` (and therefore every browser demo) caches downloaded
GGUFs in a [Cache API](https://developer.mozilla.org/en-US/docs/Web/API/Cache)
store named `nobodywho-models-v1`, keyed by URL. After the first download
on a given origin, reloads and other pages on the same origin get the
bytes back instantly from disk — no re-download, no HTTP cache eviction
surprises.

Helpers on the `Model` class:

```js
import { Model, fetchModelBytes } from 'nobodywho-js';

// Pre-populate the cache during a splash screen / onboarding step so the
// user doesn't sit through a 400 MB download when they click "chat".
await Model.preload('https://huggingface.co/.../model.gguf',
  (got, total) => console.log(`${got}/${total}`));

// Later in the app — instant if preload already ran on this origin.
const bytes = await fetchModelBytes('https://huggingface.co/.../model.gguf');
const model = await Model.loadBytes(bytes);

// Wipe all cached models (e.g. from a "clear cache" button).
await Model.clearCache();
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

The cache name is versioned (`-v1`). Bump the suffix in `setup-browser.mjs`
if the cached representation ever changes (e.g. switching from raw bytes
to a pre-decoded format) so old entries are abandoned rather than fed to
a binding that can't read them.

## How it works (and why these specific choices)

### Target: `wasm32-unknown-unknown` (not Emscripten, not WASI Preview 1/2)

- **Emscripten** also works (`cargo build --target wasm32-unknown-emscripten`
  produces a 113 MB debug .wasm) but its output isn't processable by
  `wasm-bindgen-cli` — wasm-bindgen's interpreter doesn't understand
  Emscripten's section layout. Browser distribution via wasm-bindgen + npm
  is the path that fits the rustwasm ecosystem.
- **WASI Preview 2** would be the future-proof choice, but tooling is
  still maturing and browser support is uneven.

### libc: wasi-sdk's `wasi-libc`

The Rust target `wasm32-unknown-unknown` has no libc. llama.cpp is C/C++ and
needs `<stdio.h>`, `<malloc.h>`, etc. We compile the C/C++ side targeting
`wasm32-wasip1` (via the wasi-sdk clang) and link `wasi-libc` + `libc++`
explicitly in the final cdylib link. The result is a wasm with
`wasi_snapshot_preview1` imports that the JS host polyfills via
`node:wasi` (Node) or `@bjorn3/browser_wasi_shim` (browser).

### Source-level patches to llama.cpp

The fork at [`nobodywho-ooo/llama-cpp-rs` branch `wasm`](https://github.com/nobodywho-ooo/llama-cpp-rs/tree/wasm)
patches a handful of files in `llama.cpp/common/` at build time, because
wasi-libc deliberately doesn't ship POSIX features the upstream code
assumes:

- `cpp-httplib` excised entirely (no `<net/if.h>`, no sockets).
- `arg.cpp`, `console.cpp`, `download.cpp`, `hf-cache.cpp`, `http.h`
  stripped from the source list (POSIX `<sys/syslimits.h>`, `<termios.h>`,
  HTTP).
- `_WASI_EMULATED_SIGNAL` + `_WASI_EMULATED_PROCESS_CLOCKS` compile defines
  + their matching link libs, so `<signal.h>` and `<sys/resource.h>`
  resolve to best-effort no-ops.
- `common/common.cpp` `fs_get_cache_directory` extended with an
  `#elif defined(__wasi__)` arm.
- `common/common.cpp` `set_process_priority` stubbed for `__wasi__`
  (no `PRIO_PROCESS` in wasi-libc).
- `mtmd` (multimodal) C++ skipped — depends on `vendor/miniaudio` which
  needs pthread sched APIs wasi-libc doesn't provide.

These patches should eventually land upstream in llama.cpp as
`LLAMA_BUILD_HTTPLIB=OFF`, `LLAMA_BUILD_AUDIO=OFF`, etc.

### Runtime workarounds in nobodywho

A few cfg-gates in `nobodywho/core` for wasm32:

- `tokio` features: drop `rt-multi-thread` (no OS threads).
- `ureq`, `indicatif`, `dirs`, `monty`, `bashkit`: native-only.
- Worker pattern: `std::thread::spawn` → `wasm_bindgen_futures::spawn_local`,
  `std::sync::mpsc` → `tokio::sync::mpsc::unbounded_channel`.
- Model loading: `get_model_from_bytes` constructor that bypasses the
  filesystem (`fmemopen` + `llama_model_load_from_file_ptr`).
- `Tokenizer::tokenize_text` inlines the `mtmd_default_marker` literal
  (the real C function isn't compiled).
- `Worker` n_threads hardcoded to 1 (`available_parallelism` errors on
  wasm).
- `mtmd` cargo feature on core (default on), wasm crate keeps it enabled
  for FFI declarations but the C++ implementation isn't compiled —
  `mtmd_*` symbols become wasm imports stubbed by the JS host.

And one workaround in the wasm crate itself (`js/src/lib.rs`):
- `__cxa_atexit` overridden as a no-op. `rust-lld 22.1`'s wasm driver
  doesn't accept `--mexec-model=reactor`, so the linker stays in
  "command" mode and wraps every export in `__wasm_call_ctors` +
  `__wasm_call_dtors`. The dtor walk iterates registered handlers and
  trips on a signature mismatch. Suppressing the registration entirely
  makes the dtor walk a no-op. Global destructors don't run at module
  shutdown — fine, the wasm instance lives for the JS process anyway.

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

**What's compiled in.** The Emscripten build of `llama-cpp-sys-2`
defines `MA_NO_DEVICE_IO`, `MA_NO_THREADING`, `MA_NO_ENGINE`,
`MA_NO_NODE_GRAPH`, `MA_NO_RESOURCE_MANAGER`, `MA_NO_GENERATION` and
adds `-fexceptions` to the mtmd TUs. That removes every pthread-using
piece of miniaudio (the audio device thread, the engine, the
resource-manager IO thread) while keeping the file-header sniffer and
the format decoders.

**JS API.**

```js
import { Model, Chat, Image, Audio } from 'nobodywho-js';

const modelBytes  = new Uint8Array(await (await fetch('/model.gguf')).arrayBuffer());
const mmprojBytes = new Uint8Array(await (await fetch('/mmproj.gguf')).arrayBuffer());

// Optional mmproj as a second arg — promotes the Model to multimodal.
const model = await Model.loadBytes(modelBytes, mmprojBytes);

const chat = new Chat(model, {
  systemPrompt: 'Describe the image.',
  contextSize: 4096,            // ≥ image embedding + reply
});

const imgResp = await fetch('/cat.jpg');
const img = Image.fromBytes(new Uint8Array(await imgResp.arrayBuffer()));

const answer = await (await chat.ask(['What is in this image?', img])).completed();
```

`Image.fromBytes(uint8)` / `Audio.fromBytes(uint8)` return plain JS
objects of shape `{__nbwKind: 'image' | 'audio', bytes: Uint8Array}`.
They are structured-cloneable, which is what lets them survive the
postMessage hop into the `WorkerChat`'s background worker.

The same array-of-parts shape is accepted by both `Chat.ask` (in-
process, advanced) and `WorkerChat.ask` (worker-backed, recommended).
Plain strings still work for text-only prompts — `chat.ask('hi')` is
unchanged.

**How it works under the hood** (see `js/src/syscall_imports.rs` and
`js/src/lib.rs` for code, this is just the shape):
1. JS calls `Image.fromBytes(uint8)` → tagged object.
2. `WorkerChat.ask([...])` postMessages the array to the worker.
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
- WAV audio (miniaudio's built-in WAV decoder — no `MA_NO_WAV`).
- mmproj loading from bytes via `Model.loadBytes(model, mmproj)`.
- Full mtmd chunk-tokenize / encode-chunk / decode pipeline,
  unchanged from native.

**What's known not to work / untested.**

- Audio playback / device IO. `MA_NO_DEVICE_IO` removes the
  `ma_context` / `ma_device` machinery (pthread-owned). The wasm has
  no `AudioContext` / `WebAudio` bridge — models that *generate*
  audio (TTS) would need to post their PCM samples to the page for
  Web Audio playback.
- MP3 / FLAC / Ogg / Vorbis decoding: untested end-to-end. The
  decoders are linked in (no `MA_NO_MP3` / `MA_NO_FLAC` /
  `MA_NO_VORBIS`) but no smoke run.
- Chat templates that use OpenAI-style typed-content arrays (SmolVLM,
  some Phi-3-Vision variants). Our `core/src/template.rs` emits the
  older string-with-markers shape (`<__media__>` placeholder). Qwen,
  Gemma, LLaVA, etc. use the string-with-markers convention and
  work fine; OpenAI-typed-content models would need a renderer
  update.
- Models larger than ~2 GB on disk. Node's `readFileSync` caps at
  2 GiB; for larger models the JS caller has to chunk-read into a
  Uint8Array. Wasm32's hard 4 GB ceiling also bounds model + mmproj +
  KV cache + compute buffer.

## Outstanding

- **MP3 / FLAC / Ogg audio.** Decoders are linked in but unverified.
  Worth one short test per format.
- **Browser polyfill bundling.** `setup-browser.mjs` loads
  `@bjorn3/browser_wasi_shim` from a CDN. The npm package leaves that
  as a peer dep (see `package.json.tpl`); downstream bundlers resolve
  it. Worth verifying with at least one real bundler integration
  (webpack + esbuild + vite) before 1.0.
- **Structured-output generation at runtime.** `ConstraintSpec` is
  wired through `Chat::new`'s options. Should now work on Emscripten
  (libc has `clock_gettime`, which was the blocker on
  `wasm32-unknown-unknown`); unverified.
- **Upstream llama.cpp PRs** for the build-time patches in
  `llama-cpp-sys-2` (`LLAMA_BUILD_HTTPLIB=OFF`, `LLAMA_BUILD_AUDIO=OFF`,
  `__wasi__` arms in `common/common.cpp`).
- **Push `wasm-emscripten` branch updates.** The fork's
  `wasm-emscripten` branch carries two local-only commits
  (`CMAKE_SYSTEM_PROCESSOR=wasm32` for the SIMD quant kernels;
  `MA_NO_*` + `-fexceptions` for mtmd). Until those are pushed, the
  workspace `Cargo.toml` uses a `[patch]` block pointing at a
  sibling `/Users/user/git/llama-cpp-rs` checkout.
