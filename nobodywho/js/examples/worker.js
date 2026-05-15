// Web Worker for browser-chat-worker.html. WASI + wasm bootstrap lives in
// setup-browser.mjs — by the time this import resolves, the wasm is
// loaded, ctors have run, and Model/Chat are usable. The worker then
// answers a tiny message protocol from the main thread: load-model,
// create-chat, ask.

import { Model, Chat } from './setup-browser.mjs';

// Bootstrap done — tell the main thread NOW, proactively. Don't wait for an
// `init` message: depending on browser, a message posted before the worker
// module's top-level await resolves may or may not be redelivered to a
// later `self.onmessage` handler. Sending `ready` here is the simplest
// guarantee that the main thread leaves the "Spinning up worker…" state.
self.postMessage({ type: 'ready' });

let model;
let chat;

self.onmessage = async (e) => {
  const { type, ...rest } = e.data;
  try {
    switch (type) {
      case 'init':
        // Back-compat: the main thread still posts `init` right after
        // `new Worker(...)`. Bootstrap completed before this could
        // arrive, so just re-ack — case 'ready' on the main thread is
        // idempotent (it only enables UI, no other side effects).
        self.postMessage({ type: 'ready' });
        break;
      case 'load-model':
        model = await Model.loadBytes(rest.bytes);
        self.postMessage({ type: 'model-loaded' });
        break;
      case 'create-chat':
        chat = new Chat(model, rest.options);
        self.postMessage({ type: 'chat-ready' });
        break;
      case 'ask': {
        // `askStreaming` calls the JS callback synchronously per token from
        // inside the inference loop. Posting from there is non-blocking,
        // so the main thread sees tokens as they're produced. The simpler
        // `ask` + `nextToken()` loop would batch everything until inference
        // completes, defeating the point of using a Worker.
        const t0 = performance.now();
        let count = 0;
        await chat.askStreaming(rest.prompt, (token) => {
          self.postMessage({ type: 'token', token });
          count++;
        });
        const seconds = (performance.now() - t0) / 1000;
        self.postMessage({ type: 'ask-done', count, seconds });
        break;
      }
      default:
        self.postMessage({ type: 'error', message: `unknown msg type: ${type}` });
    }
  } catch (err) {
    self.postMessage({ type: 'error', message: String(err), stack: err?.stack });
  }
};
