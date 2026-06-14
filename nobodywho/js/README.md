# nobodywho-js

WebAssembly binding for [NobodyWho](https://nobodywho.ooo) — runs local LLMs
in a browser tab (or any wasm host) via llama.cpp compiled to wasm32 with
Emscripten.

## Status: working end-to-end

Real LLM inference verified under Node and browser. The API mirrors the
Python binding — load a model from a URL or path, call the API:

**Chat** (`chat_demo.mjs`):
```js
const { default: createNobodyWhoModule } = await import('./pkg-bundler/nobodywho_js.js');
const m = await createNobodyWhoModule();

const chat = await m.Chat.create({
  modelPath: process.argv[2],   // Node: host filesystem path via NODEFS
  // modelUrl: 'https://...',   // Browser: fetched + cached via Cache API
  systemPrompt: 'You are a helpful assistant',
});

const result = await chat.ask('What is the capital of Denmark?').completed();
console.log(result);
```

`Chat` runs inference on a background pthread (via Emscripten's
pthreads support backed by `SharedArrayBuffer`), so the calling
thread stays responsive during inference. Tokens stream through a
channel and arrive in real time via async iteration.

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

const model = await m.Model.load({ modelPath: process.argv[2] });
const encoder = new m.Encoder(model, 2048);
const vec = await encoder.encode('the quick brown fox');
// -> Float32Array(384) — pass to cosineSimilarity() etc.
```

The wasm binary contains all of llama.cpp and exposes the binding's full
surface to JS via wasm-bindgen.

| Surface | Status |
|---|---|
| `Model.load({modelUrl \| modelPath, mmprojUrl \| mmprojPath})` | ✅ verified — async factory for `Encoder` / `CrossEncoder`; URL cached via Cache API, path via NODEFS |
| `Encoder.encode(text)` → `Float32Array` | ✅ verified |
| `CrossEncoder.rank(query, docs)` / `rankAndSort(...)` | ✅ verified |
| `cosineSimilarity(a, b)` → number | ✅ verified — pairs with `Encoder.encode()`; matches Python's `nobodywho.cosine_similarity` |
| `Chat.create({modelUrl \| modelPath, ...})` → `Chat` | ✅ verified — async factory. `modelUrl` streams fetch() into MEMFS with Cache API caching. `modelPath` (Node-only) mounts host dir via NODEFS. |
| `Chat.ask(prompt)` → `TokenStream` → tokens (real-time, `for await`-iterable) | ✅ verified |
| `Chat.stopGeneration()` — interrupt the current ask | ✅ verified (Node) |
| `Chat.getChatHistory()` / `setChatHistory(messages)` | ✅ verified |
| `Chat.getSystemPrompt()` / `setSystemPrompt(prompt \| null)` | ✅ verified |
| `Chat.getSamplerConfig()` / `setSamplerConfig(spec)` | ✅ verified |
| `Chat.getTemplateVariables()` / `setTemplateVariable(name, value)` / `setTemplateVariables(vars)` | ✅ verified |
| `Chat.setTools(tools)` — replace tool registry mid-session | ✅ verified |
| `Chat.reset(opts?)` — atomic clear-history + optional swap of system prompt + tools | ✅ verified |
| `Chat.resetHistory()` — clear history, preserve system prompt + tools + sampler | ✅ verified |
| `chat.free()` / GC — release the chat and its worker, like the other bindings' Drop | ✅ verified |
| `SamplerConfig` / `SamplerBuilder` / `SamplerPresets` — typed sampler API matching Python | ✅ verified |
| Structured output / Constraint via `SamplerPresets.constrainWithJsonSchema()` / `constrainWithRegex()` / `constrainWithGrammar()` | ✅ verified |
| `TokenStream.next()` / `completed()` / async-iteration via `for await` | ✅ verified |
| Multimodal vision/audio (`Image.fromBytes` / `Audio.fromBytes`, plus Node-only `fromPath`) | ✅ verified |
| Tool calling (`Tool.fromFn(...)`, `Chat.create({tools: [...]})`) | ✅ verified — sync and async JS callbacks |
| mmap-backed tensor loading (`CPU_Mapped`) | ✅ verified — strong `_mmap_js`/`_munmap_js` syscall overrides route through `FS.mmap` |

Each row above is backed by a test under `js/tests/`. To
verify locally after a build, run:

| Test | Covers |
|---|---|
| `test_emscripten.mjs` | `Model.load` + `Encoder.encode` round-trip |
| `test_onprogress.mjs` | `Model.load({modelUrl, onProgress})` download-progress callback (serves the GGUF over local HTTP) |
| `test_forawait.mjs` | `for await (const tok of chat.ask(...))` iteration |
| `test_sampler.mjs` | sampler-config knobs end-to-end |
| `test_sampler-ergo.mjs` | `SamplerBuilder` + `SamplerPresets` (core shift + sample steps) |
| `test_sampler-extra.mjs` | DRY / XTC / TypicalP / full Penalties shift steps + `dry()` / `json()` presets |
| `test_constraint.mjs` | structured output (regex / JSON Schema / lark) |
| `test_tool.mjs` | sync + async tool callbacks |
| `test_audio.mjs` | WAV / MP3 / FLAC decoder paths end-to-end |
| `test_vision.mjs` | image input through mtmd (Qwen2-VL / Gemma 3 etc.) |
| `test_stop.mjs` | `Chat.stopGeneration()` interrupting an in-flight ask |
| `test_history.mjs` | `getChatHistory` / `setChatHistory` round-trip + loaded-context use |
| `test_setters.mjs` | `setSystemPrompt` (incl. `null`) / sampler / template vars / `setTools` / `resetHistory` |
| `test_parity-extras.mjs` | `Audio.fromPath` / `Image.fromPath` / `cosineSimilarity` / `Chat.reset({systemPrompt, tools})` |
| `test_modelpath.mjs` | Node-only `Chat.create({modelPath, mmprojPath})` via NODEFS |
| `test_context-shift.mjs` | KV-cache shift when the conversation grows past `contextSize` |

Each test prints a `=== passed ===` line on success and exits 0; CI
runs them in sequence.

Native (`cargo check --workspace`) is unchanged.

## Model loading & memory optimization

Three model input modes, each optimized to minimize memory copies:

| Mode | Environment | Flow | Memory |
|---|---|---|---|
| `modelUrl` | Browser + Node | Streams `fetch()` into MEMFS via tee'd body + Cache API (downloaded once, cached on disk) | MEMFS + mmap (CPU_Mapped) |
| `modelPath` | Node only | Host directory mounted via NODEFS — llama.cpp reads directly from disk | Disk + fread (no MEMFS copy) |

**Syscall overrides.** Emscripten's `standalone.c` ships weak syscall
stubs that return `-EPERM` / `-ENOSYS`. We provide strong Rust overrides
in `js/src/syscall_imports.rs` for `openat`, `stat64`, `fstat64`,
`_mmap_js`, and `_munmap_js` — routing each through `Module.FS` /
`Module.SYSCALLS` via `js_sys::Reflect`. This makes `fopen`, `stat`,
`fstat`, and `mmap` work on MEMFS and NODEFS files.

**mmap on wasm.** The `_mmap_js` override enables llama.cpp's
`CPU_Mapped` tensor loading path (`use_mmap = true`). On MEMFS,
Emscripten's mmap allocates wasm memory and copies the file data in;
llama.cpp then maps tensors directly into that region.

**NODEFS (Node `modelPath`)** mounts the host filesystem directory via
`FS.mount(NODEFS, ...)` in `pre.js`. llama.cpp opens and reads the file
through Emscripten's VFS layer backed by Node's `fs` module. The model
file stays on disk — only tensor data enters wasm memory via fread.

**Cache API (`modelUrl`).** On first visit, the fetch response body is
tee'd: one stream goes into MEMFS, the other is stored in the Cache API
(`nobodywho-models-v1`) in the background — no slowdown vs a plain
download. On subsequent visits, `cache.match(url)` streams the model
directly from disk cache into MEMFS. Model bytes never touch the main
thread.

**Progress.** Pass an `onProgress` callback to `Model.load` / `Chat.create`
to track URL downloads — it fires per streamed chunk on the main thread as
`onProgress(loaded, total, kind)`, where `kind` is `'model'` or `'mmproj'`
and `total` is `0` when the server sent no `Content-Length`. NODEFS path
loads (`modelPath`) read from disk without streaming, so it doesn't fire
for them.

**Cache management (v1 limitation).** Browser caching is deliberately
minimal in v1: cached models persist in the `nobodywho-models-v1` Cache
Storage indefinitely — no eviction, size cap, or `clearModelCache()` API
(clear it manually via `caches.delete('nobodywho-models-v1')`); the URL is
the only cache key, so there's no revalidation if the bytes at a URL
change; and an in-flight `Model.load` can't be aborted (no `AbortSignal`).
Planned for a later release. (Node has no Cache Storage, so `modelUrl`
loads there are never cached.)

## Build pipeline

```
   nobodywho/core (Rust)  +  llama-cpp-2 fork (wasm-emscripten branch)
        |
        | cargo +nightly -Zbuild-std=std,panic_abort
        | (recompiles std with +atomics for pthreads)
        | emcc for C/C++ side, rustc for Rust side
        v
   wasm32-unknown-emscripten .wasm  (with -pthread, SharedArrayBuffer)
        |
        | patched wasm-bindgen-cli (pthreads-compatible)
        | + post-link emcc with -pthread + --js-library
        v
   pkg-bundler/
     ├── nobodywho_js.js          (Emscripten loader with pthread runtime)
     ├── nobodywho_js_bg.wasm     (linked wasm, shared memory)
     ├── nobodywho_js.wasm        (mirrored copy)
     ├── library_bindgen.js       (kept for debugging)
     └── pre.js                   (HEAP_DATA_VIEW shim, inlined by emcc)
```

## Build it yourself

### Prerequisites

You need a patched Emscripten fork, a patched wasm-bindgen fork, and
Rust nightly (for `-Zbuild-std` to recompile std with atomics).

```bash
# 1. Emscripten with the -sWASM_BINDGEN setting
#    (PR emscripten-core/emscripten#23493)
git clone -b wbg-walkingeyerobot \
  https://github.com/nobodywho-ooo/emscripten ~/emscripten-wbg
cd ~/emscripten-wbg
./bootstrap   # downloads the matching binaryen + node bundle

# 2. wasm-bindgen 0.2.122 + Emscripten/wasm64 fixes (pthread skip, MEMORY64, encodeInto)
git clone -b wasm64-emscripten \
  https://github.com/nobodywho-ooo/wasm-bindgen ~/wasm-bindgen
cargo install --path ~/wasm-bindgen/crates/cli \
  --root /tmp/wbg-patched --locked

# 3. Rust nightly + rust-src (needed for -Zbuild-std with atomics)
rustup toolchain install nightly
rustup component add rust-src --toolchain nightly
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
node examples/serve.mjs
# open http://localhost:8000/  (or /examples/browser-chat.html, etc.)
```

See `js/examples/browser-chat.html` for a chat demo,
`browser-encoder.html` for an embeddings demo, and
`browser-crossencoder.html` for a reranker demo. All load the wasm
via `createNobodyWhoModule()` and load models via
`Model.load({ modelUrl })` or `Chat.create({ modelUrl })`. Models are
cached in the Cache API (`nobodywho-models-v1` store) so subsequent
loads skip the download. `serve.mjs` sets the COOP/COEP headers
`SharedArrayBuffer` needs for pthreads; a plain static server such as
`python3 -m http.server` won't work (it can't set those headers).

## How it works (and why these specific choices)

### Target: `wasm32-unknown-emscripten`

Emscripten provides the C/C++ toolchain (libc, libc++, malloc, an
in-memory filesystem, syscall shims) that llama.cpp's C/C++ side needs.
Compiling Rust to wasm32 directly and linking against Emscripten's
libraries gives us a single wasm that hosts both halves of the binding.

The Rust side uses wasm-bindgen attributes to expose typed JS classes
(`Model`, `Chat`, `Encoder`, `CrossEncoder`, `Image`, `Audio`,
`Tool`, etc.). wasm-bindgen 0.2.122 ships Emscripten output mode
upstream; we pin a thin fork (`nobodywho-ooo/wasm-bindgen` branch
`wasm64-emscripten`) for a few Emscripten/wasm64 fixes not yet upstream:
skipping the `__heap_base` thread transform (Emscripten manages its own
pthread runtime), MEMORY64 stack-pointer/retptr handling, and a SAB-safe
`encodeInto` polyfill — until those land upstream too.

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

### Threading model

The wasm build uses Emscripten pthreads (`-pthread` on the linker,
`+atomics,+bulk-memory,+mutable-globals` Rust target features). This
enables `std::thread::spawn` on wasm — the same code path as native.
`ChatHandleAsync::new()` spawns a real pthread for inference, and
llama.cpp's ggml threadpool uses `available_parallelism()` to set
`n_threads` (maps to `navigator.hardwareConcurrency` in browser,
`os.cpus().length` in Node).

**Browser requirement:** the serving origin must set
`Cross-Origin-Opener-Policy: same-origin` and a
`Cross-Origin-Embedder-Policy` header for `SharedArrayBuffer` to be
available. `credentialless` is the easiest value — cross-origin
resources (e.g. a model fetched from HuggingFace) load without sending
their own CORP headers — but `require-corp` works too.

**Build requirement:** Rust nightly with `-Zbuild-std=std,panic_abort`
(the pre-compiled std for `wasm32-unknown-emscripten` doesn't include
atomics; `-Zbuild-std` recompiles it with the target features).

### Runtime workarounds in nobodywho

A few cfg-gates in `nobodywho/core` for wasm32:

- `tokio` features: drop `rt-multi-thread` (tokio blocks it on wasm).
  Emscripten pthreads handle compute parallelism directly.
- `ureq`, `indicatif`, `dirs`, `monty`, `bashkit`: native-only.
- `ChatHandleAsync::new()`: skips blocking `init_rx.recv()` on wasm
  (the spawned pthread needs the event loop to tick before it can start;
  blocking would deadlock).
- Model loading: `get_model_from_path` with `use_mmap(true)` via NODEFS
  or MEMFS.
- `mtmd` cargo feature on core stays enabled — Emscripten compiles it
  in (see "Multimodal status").

Strong syscall overrides in `js/src/syscall_imports.rs`:
- `__syscall_openat` — routes `fopen` through `Module.FS.open`.
- `__syscall_stat64` / `__syscall_fstat64` — routes stat through `Module.FS.stat`.
- `_mmap_js` / `_munmap_js` — routes mmap through `FS.mmap`, enabling
  llama.cpp's `CPU_Mapped` tensor loading.

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

**JS API.**

```js
const { default: createNobodyWhoModule } = await import('nobodywho-js');
const m = await createNobodyWhoModule();

const chat = await m.Chat.create({
  modelUrl: '/model.gguf',
  mmprojUrl: '/mmproj.gguf',
  systemPrompt: 'Describe the image.',
  contextSize: 4096,
});

const imgResp = await fetch('/cat.jpg');
const img = m.Image.fromBytes(new Uint8Array(await imgResp.arrayBuffer()));

const answer = await chat.ask(['What is in this image?', img]).completed();
```

`Image.fromBytes(uint8)` / `Audio.fromBytes(uint8)` return plain JS
objects of shape `{__nbwKind: 'image' | 'audio', bytes: Uint8Array}`.
The array-of-parts shape is accepted by `Chat.ask`. Plain strings
still work for text-only prompts — `chat.ask('hi')` is unchanged.

**How it works under the hood** (see `js/src/syscall_imports.rs` and
`js/src/lib.rs` for code, this is just the shape):
1. JS calls `Image.fromBytes(uint8)` → tagged object.
2. `Chat.ask([...])` extracts the bytes and calls `Module.FS.writeFile`
   via `js_sys::Reflect` to land them at a content-hashed path like
   `/home/web_user/nbw-image-<hash>.bin` in Emscripten's MEMFS.
3. The same Rust calls the existing `Prompt::push_image(&Path)`.
4. mtmd's C++ side opens the file via libc `fopen("rb")`. Libc's
   `__syscall_openat` is satisfied by a strong Rust override (also in
   `syscall_imports.rs`) that resolves the call back into
   `Module.FS.open` via `js_sys::Reflect` — completing the loop.

**What's known to work.**

- Image decoding via `stb_image`: JPEG, PNG, BMP, GIF, TGA, PSD, PIC,
  PNM. Format is sniffed from the file header by mtmd.
- Three miniaudio decoders end-to-end: **WAV, MP3, FLAC** — verified
  by `js/tests/test_audio.mjs` (Qwen3-ASR produces real
  transcripts).
- mmproj loading via `mmprojUrl` / `mmprojPath`.
- Vision encoder via mtmd (chunk-tokenize / encode-chunk / decode
  pipeline, verified against Gemma 3 + Qwen2-VL mmprojs).
- Audio-LLM mmproj encoder via mtmd (Qwen3-ASR verified end-to-end
  for WAV/MP3/FLAC — the model produces real transcripts).

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
- Models whose total working set exceeds 4 GiB **on the wasm32 build**.
  Model tensors + mmproj + KV cache + compute buffer must fit in
  wasm32's hard 4 GiB linear-memory ceiling. The sibling **wasm64
  (MEMORY64) build** (`scripts/build-pkg-emscripten-wasm64.sh` →
  `pkg-bundler-wasm64/`) lifts that to 16 GiB, multi-threaded (MEMORY64 +
  pthreads, like wasm32). One blocker — `-Zbuild-std`'s unwinder lacking a
  wasm64 `unwinder_private_data_size` const — was fixed in
  [rust-lang/rust#156573](https://github.com/rust-lang/rust/pull/156573)
  (merged 2026-06-07; a nightly ≥ 2026-06-07 needs no rustlib patch). A
  second, **pthread-specific** blocker: std's libc
  hardcodes wasm32 pthread sizes, so `pthread_attr_init` overruns std's
  `pthread_attr_t` on wasm64 and `std::thread::spawn` fails. The fix,
  [rust-lang/libc#5156](https://github.com/rust-lang/libc/pull/5156), is
  **merged upstream** (2026-06-14) — but unlike #156573 it lands in `libc`
  (a crate std vendors), not in rust itself, so it only reaches `-Zbuild-std`
  once a released libc carries it *and* a nightly's std bumps to that release.
  Until then, apply the one-time rust-src `[patch]` documented under
  **Outstanding** (the build script refuses to proceed without it). wasm32
  stays the default — wasm64 pays a download/load cost for 64-bit pointers —
  so reach for wasm64 only when a model overflows 4 GiB.
- Browser COOP/COEP headers. Pthreads are enabled but require
  `Cross-Origin-Opener-Policy: same-origin` plus a
  `Cross-Origin-Embedder-Policy` header (`credentialless` is the
  easiest; `require-corp` also works) on the serving origin for
  `SharedArrayBuffer` to be available.
- Audio: WAV/MP3/FLAC only — Ogg/Vorbis is not supported under
  Emscripten (surfaces a clean error).

## Outstanding

- **Upstream the three forks pinned by this PR.** Each carries
  changes that need to land in their respective upstream projects so we
  can drop the fork pointer. The two we maintain are publicly hosted under
  `nobodywho-ooo/` for transparency / reproducibility; the third
  (`emscripten`) is a third-party fork we only consume:
  - [`nobodywho-ooo/llama-cpp-rs` branch `wasm-emscripten`](https://github.com/nobodywho-ooo/llama-cpp-rs/tree/wasm-emscripten)
    — `CMAKE_SYSTEM_PROCESSOR=wasm32` for ggml's wasm SIMD quants, `MA_NO_*`
    defines + `-fexceptions` for mtmd. Pulled directly via `core/Cargo.toml`
    `{ git = "...", branch = "wasm-emscripten" }`. Its `llama.cpp`
    submodule tracks upstream `ggml-org/llama.cpp` (stock, unpatched).
    The wasm64 (MEMORY64) build adds target recognition + `-sMEMORY64=1` +
    `LLAMA_WASM_MEM64=ON` on the `wasm64-emscripten` branch; until that
    merges back, the workspace `Cargo.toml` `[patch]` block points both
    crates at a local clone of it (see that block's comment).
  - [`nobodywho-ooo/wasm-bindgen` branch `wasm64-emscripten`](https://github.com/nobodywho-ooo/wasm-bindgen/tree/wasm64-emscripten)
    — upstream 0.2.122 plus the Emscripten/wasm64 fixes both the crate and the cli
    need: the pthread thread-transform skip, MEMORY64 handling (the i64
    stack-pointer shim for `__wbindgen_add_to_stack_pointer` + retptr BigInt), and
    a SAB-safe `TextEncoder.encodeInto` polyfill. Both the *crate* (`Cargo.toml`
    `[patch.crates-io]`) and the js-CI *cli* (`WBG_FORK_REF`) pin this one branch,
    so they stay in lockstep.
  - [`nobodywho-ooo/emscripten` branch `wbg-walkingeyerobot`](https://github.com/nobodywho-ooo/emscripten/tree/wbg-walkingeyerobot)
    — our fork of `walkingeyerobot/emscripten`, pinned at a fixed commit for
    reproducible CI. Carries the `-sWASM_BINDGEN` flag tracked in
    [emscripten-core/emscripten#23493](https://github.com/emscripten-core/emscripten/pull/23493),
    plus a NODEFS fix to mmap GGUF models >2GB under MEMORY64 (node's
    `fs.readSync` caps a single read at 2^31-1 bytes).
    Consumed at build time via `$EMSDK_DIR` pointing at a local clone.

- **Drop the manual rust-src `[patch]` once the libc pthread-size fix
  ([rust-lang/libc#5156](https://github.com/rust-lang/libc/pull/5156)) reaches std (wasm64 only).**
  #5156 is **merged upstream** (2026-06-14, `rust-lang/libc` commit `8f9915f`,
  stable-nominated), but not yet in a released libc — so std's vendored libc
  doesn't carry it yet and the rust-src `[patch]` below stays until it does.
  libc hardcodes wasm32 pthread type sizes for `*-emscripten`; on wasm64
  `pthread_attr_t` is 88 bytes (not 44), so `pthread_attr_init` overruns the
  buffer std's `Thread::new` stack-allocates and `std::thread::spawn` fails.
  #5156 makes
  those sizes pointer-width-aware. The fix is needed **only by std's** libc:
  `-Zbuild-std` recompiles `std` from rust-src and resolves the sysroot's libc
  *separately* from this workspace, so a workspace `[patch]` would never reach
  it (confirmable with `cargo build … --unit-graph`). The app crate doesn't
  need it — its only libc use is `getuid()` — so there is **no app-side libc
  patch**; the fix is injected into the nightly's rust-src as a **one-time
  local-dev step** (the build script aborts without it):

  ```bash
  bash js/scripts/patch-rust-src-libc.sh
  ```

  That reads the exact libc version std locks, downloads it, applies the #5156
  fix, and adds `[patch.crates-io] libc = { path = … }` to rust-src's
  `library/Cargo.toml` (idempotent; restore `library/Cargo.toml.wasm64-orig` to
  undo). **CI runs this same script** (`js_ci.yml`, just before the wasm64
  build) — it's what makes the `tests-wasm64` leg pass. Verified end-to-end
  (multi-threaded `Encoder.encode` on wasm64). Once a nightly's std bumps to a
  libc that includes #5156 the fix step no-ops — delete the CI step + script then.
