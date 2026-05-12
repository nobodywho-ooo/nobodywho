# nobodywho-wasm

WebAssembly binding for [NobodyWho](https://nobodywho.ooo), letting you run
local LLMs in a browser tab via the same core engine that powers the Python,
Flutter, Godot, and Uniffi bindings.

## Status — Path B working end-to-end

```
cargo build --target wasm32-unknown-unknown -p nobodywho-wasm  ✅
wasm-bindgen --target web ... --out-dir pkg/                   ✅
```

Produces a complete npm package shape in `pkg/`:

```
pkg/
├── nobodywho_wasm.d.ts        9.1K  — TS typings for the public API
├── nobodywho_wasm.js          37K   — JS loader / wasm-bindgen glue
├── nobodywho_wasm_bg.wasm     21M   — compiled wasm (debug; ~5-7M release-stripped)
└── nobodywho_wasm_bg.wasm.d.ts
```

The TS bindings expose `Model`, `Chat`, `TokenStream`, `Encoder` with
full JSDoc and `Promise<any>` return types. JS consumers can
`import init, { Model, Chat } from './pkg/nobodywho_wasm.js'`,
`await init()`, then use the API.

### Build pipeline
```
bindgen + cc::Build (wasi-sdk clang)
  +
cmake (llama.cpp targeting wasm32-wasip1 + wasi-libc, with the
       fork's source-level patches: cpp-httplib excised, signal +
       process-clocks emulation, __wasi__ cases, mtmd C++ skipped)
                ↓
        wasm-bindgen post-processor
                ↓
        pkg/ ready for npm publish
```

Native (`cargo check --workspace`) is unchanged and builds bit-for-bit
the same as before the wasm branch.

### Alternate target: wasm32-unknown-emscripten

`cargo build --target wasm32-unknown-emscripten -p nobodywho-wasm` also
works (produces a 113 MB debug .wasm). That artifact can't be processed
by `wasm-bindgen-cli` (CLI doesn't understand Emscripten's section
layout), so it's only useful for inspecting the Emscripten output or as
a fallback for consumers who use Emscripten's own JS glue. The
`wasm32-unknown-unknown` + wasm-bindgen path above is the recommended
distribution.

### Build prerequisites

1. **wasi-sdk** for the C/C++ side of llama.cpp:
   ```bash
   curl -L https://github.com/WebAssembly/wasi-sdk/releases/download/wasi-sdk-33/wasi-sdk-33.0-arm64-macos.tar.gz | tar -xz -C ~
   export WASI_SDK_PATH=~/wasi-sdk-33.0-arm64-macos
   ```
   (Or `/opt/wasi-sdk` if you prefer a system-wide install — the build
   script probes that path automatically.)

2. **Rust wasm target**:
   ```bash
   rustup target add wasm32-unknown-unknown
   ```

3. **wasm-bindgen-cli** (must match the `wasm-bindgen` crate version
   you're building with — currently 0.2.121):
   ```bash
   cargo install wasm-bindgen-cli --version 0.2.121
   ```

### Build steps

```bash
cd nobodywho/wasm
WASI_SDK_PATH=~/wasi-sdk-33.0-arm64-macos \
  cargo build --target wasm32-unknown-unknown --release

wasm-bindgen --target web \
  ../target/wasm32-unknown-unknown/release/nobodywho_wasm.wasm \
  --out-dir pkg/

# pkg/ is the npm-publishable directory
```

### What's wired up

The `llama-cpp-2` fork at
[`nobodywho-ooo/llama-cpp-rs` `wasm`](https://github.com/nobodywho-ooo/llama-cpp-rs/tree/wasm)
inherits Marek's llguidance/EOS-fix patches and adds Asbjørn's Emscripten
support (cherry-picked from his branch). Specifically:

- `TargetOs::Emscripten` variant + sysroot/toolchain auto-detection from
  `emcc --cflags` and `which emcc`.
- Bindgen configured with the real sysroot + `--target=wasm32-unknown-emscripten`
  + `-fvisibility=default` (workaround for bindgen #1941).
- `cc` wrapper-shim build uses `em++` with `-fwasm-exceptions` (native wasm EH).
- cmake disables every GPU backend (`GGML_VULKAN`/`GGML_CUDA`/`GGML_METAL`/etc.),
  forces `BUILD_SHARED_LIBS=OFF` and `LLAMA_WASM_MEM64=OFF`.

See `WASM.md` in the fork for the full build details and remaining gaps.

### Outstanding work — pick A or B

#### Path A — drop wasm-bindgen, use Emscripten's JS glue (works today)

Keep the existing `wasm32-unknown-emscripten` build. **Replace
`#[wasm_bindgen]` with `#[no_mangle] extern "C"`** in `src/lib.rs`,
marshaling types manually (string pointers via `Box::into_raw`, byte
arrays as `(*mut u8, usize)`, async results as callback handles).
Emscripten emits a JS loader via `MODULARIZE=1` that handles all libc
imports automatically.

Effort: ~1 day. Less type-safe than wasm-bindgen, but builds on the
working pipeline and ships a usable npm package immediately.

#### Path B — wasm32-unknown-unknown + wasi-sdk (C++ side done, Rust mtmd gating remains)

The fork's `wasm` branch now has a full `wasm32-unknown-unknown` build
path: `TargetOs::WasmUnknown` variant, wasi-sdk detection, parallel
bindgen/cmake/cc config, and source-level patches to llama.cpp itself
(cpp-httplib excision, `arg.cpp`/`console.cpp` removal, signal +
process-clocks emulation, `fs_get_cache_directory` `__wasi__` case,
`set_process_priority` no-op, mtmd's miniaudio skipped because it needs
pthread sched APIs wasi-libc doesn't ship).

**End-to-end C++ build works** for wasm32-unknown-unknown. The remaining
blocker is in **`nobodywho/core` itself**: it imports
`llama_cpp_2::mtmd::{MtmdInputChunks, MtmdBitmap, MtmdContext, ...}` and
those types now don't exist on wasm32 (since the fork's mtmd Rust module
is also gated off — the FFI symbols aren't there because the mtmd C++
isn't compiled).

`core/src/chat.rs` and `core/src/tokenizer.rs` use these types
structurally (as struct fields, enum variants like `Asset::Image`,
`TokenizerChunk::Image`/`Audio`, and `bitmaps: IndexMap<ChunkId,
MtmdBitmap>` on the chat worker state). Gating them out for wasm is a
mechanical-but-substantial refactor:

```
nobodywho/core/src/llm.rs       — gate MtmdInputChunks import + 1 field
nobodywho/core/src/template.rs  — gate MtmdBitmap import + 1 field
nobodywho/core/src/errors.rs    — gate one error variant
nobodywho/core/src/chat.rs      — gate ~5 places (worker state, methods)
nobodywho/core/src/tokenizer.rs — gate ~30 places (Chunk variants, all
                                  match arms, multimodal init paths)
```

Roughly 1–2 hours of careful cfg-attribute application. Either
`#[cfg(not(target_arch = "wasm32"))]` everywhere or adding a `mtmd`
cargo feature to core (cleaner long-term — wasm just opts out).

Effort estimate to finish B: ~half a day. Plus ongoing maintenance of
the llama.cpp source patches as llama.cpp evolves (these should
eventually go upstream — `LLAMA_BUILD_HTTPLIB=OFF`, `LLAMA_BUILD_AUDIO=OFF`).

#### Path C — `wasm32-wasip2` + component model

Future-proof but bleeding-edge tooling; not recommended for shipping
this year.

### Status of the fork's `wasm` branch

The `wasm32-unknown-emscripten` build works end-to-end (113 MB .wasm
artifact). The parallel `wasm32-unknown-unknown` path is partial as
described above.

2. **End-to-end smoke test.** Embed a small GGUF as bytes, call
   `Model.loadBytes`, run one `Chat.ask`, log a token. Confirms the full
   stack (FFI → llama.cpp → sampler → channel pump) works in a browser.

3. **Release-mode build size.** Debug is 113 MB. Release should be
   ~10–20 MB. Verify with `cargo build --release --target ...`. Brotli
   compression brings the served size down further.

### Done in earlier commits

- ✅ `LlamaModel::load_from_buffer` wraps `llama_model_load_from_file_ptr`
  via libc `fmemopen`. `nobodywho::llm::get_model_from_bytes` in core
  exposes it. `Model.loadBytes(uint8Array)` in this binding wires it
  through.
- ✅ Worker refactor: `std::thread::spawn` + `std::sync::mpsc` swapped for
  `tokio::sync::mpsc::unbounded_channel` on both targets;
  `wasm_bindgen_futures::spawn_local` on wasm vs `std::thread::spawn` +
  `blocking_recv` on native. See `core/src/chat.rs`, `encoder.rs`,
  `crossencoder.rs`.

### Step 2a — `core/` dependency gates ✅ done

- `core/Cargo.toml` — `tokio` split: `rt-multi-thread` on native, plain
  `rt` on wasm. `ureq` (raw sockets, not available in browser) and
  `indicatif` (no terminal) gated to non-wasm. `dirs` gated to non-android,
  non-wasm. `monty` (Python interpreter) and `bashkit` (virtual bash) gated
  to non-wasm.
- `core/src/llm.rs` — `default_progress_callback` and the entire
  model-loader infrastructure (`get_model`, `get_model_async`,
  `download_*`, `ParsedModelPath`) gated to non-wasm.
- `core/src/tool_calling/mod.rs` — `Tool::python` and `Tool::bash` gated
  to non-wasm.
- `grammar/gbnf` — `jsonschema` default features disabled (no HTTP/file
  resolution needed), drops the entire `reqwest`/`hyper`/`mio` chain.

### Step 2b — Worker refactor (not yet done)

The Worker pattern uses `std::thread::spawn` + blocking `std::sync::mpsc::recv`
in five places:
- `core/src/chat.rs:307` (`ChatHandle::new`)
- `core/src/chat.rs:576` (`ChatHandleAsync::new`)
- `core/src/encoder.rs:34` (`EncoderAsync::new`)
- `core/src/crossencoder.rs:47` (`CrossEncoderAsync::new`)
- `core/src/llm.rs:317` (`get_model_async`)

`WorkerGuard` (`core/src/llm.rs:828`) stores a `std::thread::JoinHandle`.

wasm32 has no OS threads. Each of these needs a cfg-gated alternative that
uses `wasm_bindgen_futures::spawn_local` and `tokio::sync::mpsc` async
channels instead. The `WorkerGuard` needs to grow a cfg-gated variant that
holds a cancellation token instead of a `JoinHandle`. This is a real design
change — needs maintainer alignment on whether wasm uses cooperative
single-threaded inference (blocks the main thread during decode) or Web
Workers for true parallelism.

### Step 2c — Bytes-based model loading (not yet done)

`core/src/llm.rs` `get_model` paths go through file I/O and `ureq`
downloads. A new `Model::load_bytes(Vec<u8>) -> Result<Model, _>`
constructor is needed for wasm (and is useful on native too — tests
already have GGUF bytes in memory). The scaffold's
`Model::loadBytes` JS API points to this.

## What's exposed

Mirrors the Python binding's surface, async-only:

| Class | Methods |
|---|---|
| `Model` | `Model.loadBytes(uint8Array)` *(blocked on Step 2)* |
| `Chat` | `new Chat(model, options)`, `ask(prompt)`, `reset(systemPrompt?)`, `resetHistory()` |
| `TokenStream` | `nextToken()`, `completed()` |
| `Encoder` | `new Encoder(model, nCtx?)`, `encode(text)` |

Not yet wrapped (will be added in follow-up PRs): `CrossEncoder`,
`Constraint` (structured output), tool calling, multimodal assets. See the
end of `src/lib.rs` for the full out-of-scope list and the reason each is
deferred.

## Build (once Steps 1 and 2 are done)

```bash
# One-time setup
cargo install wasm-pack
rustup target add wasm32-unknown-unknown

# Build
cd nobodywho/wasm
wasm-pack build --target web
```

Output goes to `nobodywho/wasm/pkg/`. That directory is the publishable npm
package — `package.json`, `.wasm` artifact, JS glue, and `.d.ts` typings.

## Use (browser, ES modules)

```html
<script type="module">
  import init, { Model, Chat } from './pkg/nobodywho_wasm.js';

  await init();

  const buf = await (await fetch('./tinyllama.gguf')).arrayBuffer();
  const model = await Model.loadBytes(new Uint8Array(buf));

  const chat = new Chat(model, {
    contextSize: 2048,
    systemPrompt: 'You are a concise assistant.',
  });

  const stream = await chat.ask('Why is the sky blue?');
  let tok;
  while ((tok = await stream.nextToken()) !== undefined) {
    document.body.append(tok);
  }
</script>
```

## Caveats

- **Memory ceiling.** `wasm32` is capped at 4 GB. Models larger than that
  need wasm64 (`memory64` proposal — Chrome 119+, Firefox 134+), which our
  toolchain doesn't enable yet. Practically: stick to Q4/Q5 quantizations of
  ≤7B-parameter models for now.
- **No GPU.** WebGPU acceleration in llama.cpp is experimental upstream and
  not part of this binding's first release. CPU only.
- **Cross-origin isolation.** If/when we enable wasm threads via
  `SharedArrayBuffer`, the hosting page needs `Cross-Origin-Opener-Policy:
  same-origin` and `Cross-Origin-Embedder-Policy: require-corp` headers. The
  first release is single-threaded.
- **Bundle size.** llama.cpp compiled with Emscripten is several MB. Plan to
  serve the `.wasm` over HTTPS with `Content-Encoding: br` (Brotli) and
  cache aggressively.
