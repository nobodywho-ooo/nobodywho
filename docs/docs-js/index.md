---
title: Getting started
description: How to set up NobodyWho in JavaScript — in the browser or in Node
sidebar_position: 0
---

NobodyWho runs local LLMs directly from JavaScript — in a browser tab or in Node — with no servers, API keys, or native add-ons. Under the hood it's [llama.cpp](https://github.com/ggml-org/llama.cpp) compiled to WebAssembly, so inference happens on the user's own machine.

## Install

```bash
npm install @nobodywho/js
```

The package ships the wasm binary and works with any bundler (Vite, webpack, etc.) and in Node 20+.

## Loading the module

Everything starts by instantiating the wasm module. The default export is an async factory:

```js
import createNobodyWhoModule from '@nobodywho/js';

const m = await createNobodyWhoModule();
```

`m` exposes the API — `Chat`, `Model`, `Encoder`, `CrossEncoder`, `Tool`, `Image`, `Audio`, and the sampler helpers.

## Your first chat

Models load from any `https://` URL (including Hugging Face) in the browser, or from a local file path in Node. If you don't have a model in mind, try [this one](https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](/docs/model-selection).

```js
import createNobodyWhoModule from '@nobodywho/js';
const m = await createNobodyWhoModule();

const chat = await m.Chat.create({
  // Browser: fetch + cache the model from a URL
  modelUrl: 'https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
  // Node: load from a local path instead
  // modelPath: '/path/to/model.gguf',
  systemPrompt: 'You are a helpful assistant.',
});

const reply = await chat.ask('Is water wet?').completed();
console.log(reply); // Yes, water is wet!
```

That's the whole "hello world". Tokens can also be streamed as they arrive — see [Chat](./chat).

## Browser vs Node

The same API runs in both; only model loading differs:

|  | Browser | Node |
|---|---|---|
| Model source | `modelUrl` (any `https://` URL) | `modelPath` (local file) or `modelUrl` |
| Caching | downloaded once, kept in the Cache API | read from disk |
| Threads | Web Workers (needs cross-origin isolation — see below) | `worker_threads` |

## Browser requirement: cross-origin isolation

Inference runs on background threads backed by `SharedArrayBuffer`. Browsers only expose `SharedArrayBuffer` to **cross-origin isolated** pages, so the origin serving your app **must** send two response headers:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: credentialless
```

Without them, `self.crossOriginIsolated` is `false` and the model won't load. `credentialless` is the easiest value — cross-origin resources (such as a model fetched from Hugging Face) load without extra cooperation — but `require-corp` also works. A plain static server like `python3 -m http.server` *can't* set these headers; the repo ships a tiny [`examples/serve.mjs`](https://github.com/nobodywho-ooo/nobodywho/tree/main/nobodywho/js/examples) that does, and any dev server (Vite, `npx serve`, …) can be configured to as well.

> Node has no such requirement — `SharedArrayBuffer` is always available there.

## Requirements

- **Node:** 20 or newer.
- **Browser:** any modern browser, served cross-origin isolated (above). Inference is CPU-only via wasm, and the total working set (model + context) must fit under wasm32's hard 4 GiB memory ceiling — so favor small, quantized models.

## Feedback & Contributions

We welcome your feedback and ideas!

- Bug Reports & Improvements: open an issue on our [Issues](https://github.com/nobodywho-ooo/nobodywho/issues) page.
- Feature Requests & Questions: join the discussion on our [Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) page.
