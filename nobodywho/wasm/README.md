# nobodywho-wasm

WebAssembly binding for [NobodyWho](https://nobodywho.ooo) — runs local LLMs
in a browser tab (or any wasm host) via llama.cpp compiled to wasm32.

## Status: working end-to-end

Real LLM inference verified under Node, both Encoder and Chat:

**Embedding** with a 35 MB BGE-small GGUF:
```
$ node wasm/examples/run.mjs --encode /tmp/bge-small.gguf "Hello"
  ✓ model loaded                 ✓ encoder created
  ✓ embedding generated: 384 dimensions
  first 8: [-0.6244, -0.5940, 0.5545, -0.6085, -0.1348, 0.1800, 0.6621, 0.3490]
```

**Chat** with a 379 MB Qwen 2.5 0.5B Instruct GGUF:
```
$ node wasm/examples/run.mjs /tmp/Qwen2.5-0.5B-Instruct-Q4_K_M.gguf "Hello"
  ✓ model loaded                 ✓ chat created
Asking: "Hello"
Response: Hello! How can I assist you today? If you have any questions
          or need help with something, feel free to ask.
  ✓ produced 25 tokens
```

The wasm binary contains all of llama.cpp (~9.5 MB release, ~21 MB debug)
and exposes the binding's full surface to JS via wasm-bindgen.

| Surface | Status |
|---|---|
| `Model.loadBytes(uint8Array)` | ✅ verified — loads GGUF into a real `LlamaModel` via `fmemopen` + `llama_model_load_from_file_ptr` |
| `Encoder.encode(text)` → `Float32Array` | ✅ verified |
| `Chat.ask(prompt)` → `TokenStream` → tokens | ✅ verified |
| `TokenStream.nextToken()` / `completed()` | ✅ verified |
| Multimodal (`MtmdBitmap` etc.) | not exposed — mtmd C++ doesn't compile against wasi-libc; the wasm has unresolved `mtmd_*` imports that JS replaces with stubs |
| Structured output (`Constraint`) | not yet wired up to the JS surface (works inside core, no JS wrapping yet) |

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
     ├── nobodywho_wasm.js          (entry — calls __wbg_set_wasm)
     ├── nobodywho_wasm_bg.js       (Chat/Model/Encoder classes + glue)
     ├── nobodywho_wasm_bg.wasm     (compiled wasm)
     ├── nobodywho_wasm.d.ts        (TS typings)
     └── nobodywho_wasm_bg.wasm.d.ts
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
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.121
```

### Build

```bash
cd nobodywho

WASI_SDK_PATH=$WASI_SDK_PATH \
  cargo build --target wasm32-unknown-unknown --release -p nobodywho-wasm

wasm-bindgen --target bundler \
  target/wasm32-unknown-unknown/release/nobodywho_wasm.wasm \
  --out-dir wasm/pkg-bundler/
```

The `pkg-bundler/` directory is the npm-publishable artifact (minus a
hand-written `package.json` — see `wasm/package.json.tpl`).

## Run it

### Under Node (uses `node:wasi` for WASI imports)

```bash
# Smoke test (no model):
node wasm/examples/run.mjs

# Real embedding inference with a GGUF:
node wasm/examples/run.mjs --encode /path/to/embedding-model.gguf "your text"

# Chat (when you have a chat-style GGUF):
node wasm/examples/run.mjs /path/to/chat-model.gguf "your prompt"
```

### In a browser (uses `@bjorn3/browser_wasi_shim`)

```bash
cd nobodywho/wasm
python3 -m http.server 8000
# open http://localhost:8000/examples/browser.html
# pick a GGUF, click Run
```

See `wasm/examples/browser.html` for a complete browser demo that loads the
wasm, polyfills WASI via `@bjorn3/browser_wasi_shim`, and runs
`Encoder.encode` on a user-uploaded GGUF.

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

And one workaround in the wasm crate itself (`wasm/src/lib.rs`):
- `__cxa_atexit` overridden as a no-op. `rust-lld 22.1`'s wasm driver
  doesn't accept `--mexec-model=reactor`, so the linker stays in
  "command" mode and wraps every export in `__wasm_call_ctors` +
  `__wasm_call_dtors`. The dtor walk iterates registered handlers and
  trips on a signature mismatch. Suppressing the registration entirely
  makes the dtor walk a no-op. Global destructors don't run at module
  shutdown — fine, the wasm instance lives for the JS process anyway.

## Outstanding

- **Chat smoke test.** Encoder API verified; Chat uses the same worker
  plumbing but needs a chat-style GGUF to actually invoke. The 35 MB
  bge-small only does embeddings.
- **Release-publishable npm package.** `pkg-bundler/` is the right shape;
  needs a `package.json` (template at `wasm/package.json.tpl`) and a
  `prepublish` step that does the cargo build + wasm-bindgen, plus CI.
- **Browser polyfill bundling.** The `browser.html` example loads
  `@bjorn3/browser_wasi_shim` from a CDN. For npm distribution we'd
  bundle it (or add it as a peer dep, as the template `package.json`
  already does).
- **`Constraint` / structured output API surface.** Wired up in core, not
  yet exposed from `wasm/src/lib.rs`. Mostly a serde-wasm-bindgen
  pass-through.
- **Upstream llama.cpp PRs** for the build-time patches.
