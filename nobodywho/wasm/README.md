# nobodywho-wasm

WebAssembly binding for [NobodyWho](https://nobodywho.ooo), letting you run
local LLMs in a browser tab via the same core engine that powers the Python,
Flutter, Godot, and Uniffi bindings.

## Status: scaffold, does not build to wasm yet

This crate establishes the binding's public API and workspace integration. It
**compiles cleanly on native** (so `cargo check` at the workspace root keeps
working), but `wasm-pack build --target web` will fail until two upstream
prerequisites land:

1. **`llama-cpp-2` needs a wasm32 build path.**
   `nobodywho/core/Cargo.toml:15` pins the `marek-hradil/llama-cpp-rs` fork.
   The fork's `build.rs` invokes CMake against llama.cpp's C++ source, which
   has no wasm32 toolchain configured. The plan (Option 1 / Step 1 of the
   WASM rollout) is to maintain our own fork on top of Marek's that adds an
   Emscripten cmake branch and feature-gates out `openmp`, `mtmd`, `cuda`,
   `vulkan`, and `metal` for `target_arch = "wasm32"`.

2. **`nobodywho/core` needs `cfg(target_arch = "wasm32")` gating.**
   Specifically:
   - `core/Cargo.toml:22-27` — split tokio features so wasm gets `rt` only,
     not `rt-multi-thread`.
   - `core/Cargo.toml:40` — gate the `ureq` HTTP download dependency to
     non-wasm targets; add a bytes-based `Model::load_bytes` API for wasm.
   - `core/src/chat.rs:307`, `core/src/llm.rs` Worker setup — replace
     `std::thread::spawn` with `wasm_bindgen_futures::spawn_local` on wasm.
   - `core/src/llm.rs` `default_progress_callback` (`indicatif`) — gate to
     non-wasm.

Neither change touches this crate; both unblock it.

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
