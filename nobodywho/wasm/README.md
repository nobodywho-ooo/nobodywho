# nobodywho-wasm

WebAssembly binding for [NobodyWho](https://nobodywho.ooo), letting you run
local LLMs in a browser tab via the same core engine that powers the Python,
Flutter, Godot, and Uniffi bindings.

## Status

The wasm32 build path is **real but untested end-to-end**. `cargo check
--target wasm32-unknown-emscripten -p nobodywho-wasm` exercises the full
toolchain (bindgen + cc + cmake via Emscripten) and panics with a clear
`Could not detect Emscripten sysroot. Ensure 'emcc' is on PATH...`
message unless `emcc` is installed. Native (`cargo check --workspace`)
is unaffected.

### Build prerequisites

The wasm32 target is **`wasm32-unknown-emscripten`** (not
`wasm32-unknown-unknown`). To produce a `.wasm` artifact you need:

```bash
# Install emsdk (one-time):
git clone https://github.com/emscripten-core/emsdk.git
cd emsdk
./emsdk install latest
./emsdk activate latest
source ./emsdk_env.sh   # adds emcc, em++ to PATH

# Add the rustc target:
rustup target add wasm32-unknown-emscripten

# Build:
cd nobodywho/wasm
cargo build --target wasm32-unknown-emscripten --release
# or, for the JS-bundle output:
wasm-pack build --target web
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

### Outstanding work

1. **End-to-end build verification.** Asbjørn's commits are marked
   "still untested". Someone with `emsdk` activated needs to run
   `cargo build --target wasm32-unknown-emscripten` and shake out any
   remaining flag tuning.
2. **`Model::load_from_bytes`.** The current `Model::loadBytes` in
   `src/lib.rs` returns a placeholder `JsError`. Wiring it up needs a
   `LlamaModel::load_from_buffer` wrapper in the fork (Step 2c below).
3. **Worker refactor.** `core/src/chat.rs:307` and similar use
   `std::thread::spawn`, which wasm32 doesn't have. Needs cfg-gated
   `wasm_bindgen_futures::spawn_local` path.

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
