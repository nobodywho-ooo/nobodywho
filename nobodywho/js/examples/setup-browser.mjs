// Browser bootstrap for the wasm bundle. Importing this module loads the
// wasm from ../pkg-bundler/, wires up WASI via @bjorn3/browser_wasi_shim,
// and re-exports the binding classes. HTML / Worker code just does:
//
//   import { Model, Chat } from './setup-browser.mjs';
//
// Mirrors setup.mjs (Node) but with the browser-side WASI polyfill
// (`node:wasi` only exists in Node). Works both on the main thread and
// inside a Web Worker — `import.meta.url` resolves relative paths
// correctly in either context.

import { WASI, OpenFile, File, ConsoleStdout } from
  'https://esm.sh/@bjorn3/browser_wasi_shim@0.4.1';

const pkgDir = new URL('../pkg-bundler/', import.meta.url);
const [wasmBytes, bg] = await Promise.all([
  fetch(new URL('nobodywho_js_bg.wasm', pkgDir)).then((r) => r.arrayBuffer()),
  import(new URL('nobodywho_js_bg.js', pkgDir).href),
]);

// Empty stdio + no preopens = no host filesystem visibility, matching
// what node:wasi gives us on the Node side.
const wasi = new WASI([], [], [
  new OpenFile(new File([])),
  ConsoleStdout.lineBuffered(() => {}),
  ConsoleStdout.lineBuffered(() => {}),
]);

// Imports we don't actually exercise: mtmd_* (multimodal C++ skipped from
// the wasi-libc build), _Unwind_* (legacy exception ABI), dlclose. Throw
// on call so accidental use shows up loud during development.
const envStubs = new Proxy({}, {
  get: (_t, name) => (...args) => {
    throw new Error(`unresolved env.${String(name)}(${args.join(', ')})`);
  },
});

const inst = await WebAssembly.instantiate(await WebAssembly.compile(wasmBytes), {
  './nobodywho_js_bg.js': bg,
  env: envStubs,
  wasi_snapshot_preview1: wasi.wasiImport,
});

// `wasi.initialize` runs `_initialize` (libc + libc++ static ctors), then
// `__wbindgen_start` does wasm-bindgen's own startup. Each runs once.
wasi.initialize(inst);
bg.__wbg_set_wasm(inst.exports);
if (inst.exports.__wbindgen_start) inst.exports.__wbindgen_start();
bg.init();

// The raw wasm-bindgen `Chat` is re-exported as `ChatRaw` so the new
// worker-backed `Chat` class below can own the bare name. `worker.js` (which
// already runs inside a Web Worker, where blocking inference is fine) uses
// `ChatRaw` directly. Application code uses `Chat`.
export const { Model, Chat: ChatRaw, Encoder, CrossEncoder, TokenStream } = bg;

// Versioned Cache API store. Bump the suffix if the cached representation
// ever changes (e.g. switch from raw bytes to a pre-decoded format) so old
// entries are abandoned rather than fed to a binding that can't read them.
const MODEL_CACHE_NAME = 'nobodywho-models-v1';

// Stable for one page load; new value on every page reload. Used to
// cache-bust the worker URL — see `Chat` constructor below.
const WORKER_CACHE_BUST = Date.now();

// Open the model cache, or return null if the Cache API isn't usable in
// this context (insecure http origin, file://, sandboxed iframe without
// the right allowances). Callers fall through to a plain fetch.
async function openModelCache() {
  if (typeof caches === 'undefined') return null;
  try {
    return await caches.open(MODEL_CACHE_NAME);
  } catch {
    return null;
  }
}

// Fetch a GGUF from a URL and resolve to its bytes, reporting progress.
//
// The native binding has a `huggingface:` / `https://` path that downloads and
// caches the model on disk, but that codepath is `cfg(not(wasm32))` — `ureq`
// has no browser equivalent and there's no filesystem to cache into. So the
// browser-side equivalent is fetch() + Cache API. The cache survives reloads
// and is keyed by URL within one origin; cross-origin sharing isn't possible
// (browser storage is origin-partitioned by design).
//
// `onProgress(downloaded, total)` mirrors the native `on_download_progress`
// signature. `total` is 0 when the server doesn't send `Content-Length`.
// On a cache hit `onProgress` is called once with `(size, size)` so UIs that
// only update on progress events don't appear stuck.
export async function fetchModelBytes(url, onProgress) {
  const cache = await openModelCache();
  if (cache) {
    const cached = await cache.match(url);
    if (cached) {
      // Cache hit. arrayBuffer() returns a transferable ArrayBuffer; wrap
      // in Uint8Array to match the miss path's return shape.
      const buf = await cached.arrayBuffer();
      const bytes = new Uint8Array(buf);
      if (onProgress) onProgress(bytes.byteLength, bytes.byteLength);
      return bytes;
    }
  }

  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`fetch ${url}: HTTP ${response.status} ${response.statusText}`);
  }
  const totalHeader = response.headers.get('content-length');
  const total = totalHeader ? parseInt(totalHeader, 10) : 0;

  // Stream the body so we can report progress. response.arrayBuffer() would
  // hide progress until the whole download lands.
  const reader = response.body.getReader();
  const chunks = [];
  let downloaded = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    chunks.push(value);
    downloaded += value.byteLength;
    if (onProgress) onProgress(downloaded, total);
  }

  const bytes = new Uint8Array(downloaded);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }

  // Populate the cache for next time. Swallow failures (quota exceeded,
  // private-mode restrictions, etc.) — the caller has the bytes either way.
  // Construct a fresh Response from the assembled bytes; the original
  // streaming Response has already been consumed by getReader.
  if (cache) {
    try {
      await cache.put(url, new Response(bytes, {
        headers: {
          'Content-Length': String(downloaded),
          'Content-Type': 'application/octet-stream',
        },
      }));
    } catch (e) {
      console.warn('[nobodywho] cache.put failed; continuing without cache:', e);
    }
  }
  return bytes;
}

// Pre-populate the model cache without loading the model into wasm. Useful
// during splash screens / onboarding so the user doesn't sit on a long
// download when they actually click "chat". No-op (just performs the fetch)
// when the Cache API is unavailable.
Model.preload = async function preload(url, onProgress) {
  await fetchModelBytes(url, onProgress);
};

// Delete the model cache. Returns true if a cache existed and was removed,
// false otherwise (no cache, or Cache API unavailable). Useful for "clear
// cached models" buttons in app settings.
Model.clearCache = async function clearCache() {
  if (typeof caches === 'undefined') return false;
  try {
    return await caches.delete(MODEL_CACHE_NAME);
  } catch {
    return false;
  }
};

// Stream of tokens produced by `Chat.ask(...)`. Supports two shapes:
//
//   for await (const token of chat.ask(prompt)) { ... }   // streaming
//   const fullText = await chat.ask(prompt).completed();  // full response
//
// Both views see the same underlying token sequence. `completed()` always
// resolves to the full text — iterating doesn't consume from it. Mirrors
// the shape of Python's `TokenStream` (`__iter__` + `.completed()`).
class WorkerTokenStream {
  constructor() {
    this._buffer = [];          // tokens not yet pulled by next()
    this._fullText = '';        // accumulated text for completed()
    this._done = false;
    this._error = null;
    this._waiter = null;        // resolver for a pending next()
    this._completedWaiters = []; // resolvers waiting on completed()
  }

  _onToken(token) {
    this._fullText += token;
    if (this._waiter) {
      const w = this._waiter;
      this._waiter = null;
      w.resolve({ value: token, done: false });
    } else {
      this._buffer.push(token);
    }
  }

  _onDone() {
    this._done = true;
    if (this._waiter) {
      const w = this._waiter;
      this._waiter = null;
      w.resolve({ value: undefined, done: true });
    }
    for (const r of this._completedWaiters) r.resolve(this._fullText);
    this._completedWaiters = [];
  }

  _onError(err) {
    this._error = err;
    if (this._waiter) {
      const w = this._waiter;
      this._waiter = null;
      w.reject(err);
    }
    for (const r of this._completedWaiters) r.reject(err);
    this._completedWaiters = [];
  }

  [Symbol.asyncIterator]() { return this; }

  next() {
    if (this._error) return Promise.reject(this._error);
    if (this._buffer.length > 0) {
      return Promise.resolve({ value: this._buffer.shift(), done: false });
    }
    if (this._done) return Promise.resolve({ value: undefined, done: true });
    return new Promise((resolve, reject) => {
      this._waiter = { resolve, reject };
    });
  }

  completed() {
    if (this._error) return Promise.reject(this._error);
    if (this._done) return Promise.resolve(this._fullText);
    return new Promise((resolve, reject) => {
      this._completedWaiters.push({ resolve, reject });
    });
  }
}

// User-facing chat. Spawns `worker.js` under the hood so inference runs off
// the main thread, but the API mirrors the native Python binding's shape:
//
//   const chat = await Chat.create({
//     modelUrl: '...gguf',
//     systemPrompt: 'You are a helpful assistant',
//     templateVariables: { enable_thinking: false },
//   });
//   for await (const tok of chat.ask('hi')) console.log(tok);
//
// `Chat.create` accepts the same chat-time options as `new ChatRaw(model, opts)`
// (contextSize, systemPrompt, templateVariables, constraint), plus either
// `modelUrl` (string, fetched via fetchModelBytes — picks up the Cache API
// store) or `modelBytes` (pre-fetched Uint8Array). Pass `onDownloadProgress`
// to surface fetch progress when using `modelUrl`.
export class Chat {
  static async create({ modelUrl, modelBytes, onDownloadProgress, ...chatOptions } = {}) {
    if (!modelUrl && !modelBytes) {
      throw new Error('Chat.create: pass modelUrl or modelBytes');
    }
    const chat = new Chat();
    try {
      await chat._init({ modelUrl, modelBytes, onDownloadProgress, chatOptions });
    } catch (err) {
      chat.terminate();
      throw err;
    }
    return chat;
  }

  constructor(...args) {
    // Catch the wrong call site early. The raw wasm-bindgen Chat takes
    // `(model, options)`; this worker-backed wrapper takes no args (use
    // `Chat.create({...})`). If a stale cached `worker.js` from before the
    // ChatRaw rename calls `new Chat(model, options)` against the new
    // setup-browser.mjs, you'd otherwise get an opaque "askStreaming is not
    // a function" later. Throwing here points at the real cause.
    if (args.length > 0) {
      throw new TypeError(
        'Chat: use `await Chat.create({ modelUrl, ... })` instead of `new Chat(model, options)`. ' +
        'If you want the raw wasm-bindgen Chat class (e.g. inside a Worker), import `ChatRaw` instead. ' +
        '(Often this means a cached worker.js is stale — hard-reload to refetch.)',
      );
    }
    // Cache-bust the worker URL. Browsers cache Web Worker scripts
    // aggressively (using `Last-Modified` heuristic freshness from
    // python -m http.server, the old worker.js sticks around across
    // hard-reloads). The timestamp is generated once per setup-browser.mjs
    // load — page reload → new timestamp → fresh worker.js. When this
    // ships in the npm package, a proper bundler will replace this with
    // a content-hashed filename and the param can go away.
    this._worker = new Worker(
      new URL(`./worker.js?t=${WORKER_CACHE_BUST}`, import.meta.url),
      { type: 'module' },
    );
    this._currentStream = null;
    this._terminated = false;
    this._ready = this._waitForType('ready');
    this._worker.addEventListener('message', (e) => this._onMessage(e.data));
    this._worker.addEventListener('error', (e) => this._onWorkerError(e));
  }

  // Wait for a specific message type from the worker. Resolves on match,
  // rejects on `error`. Removes its own listener when it fires. Used for
  // the linear init handshake (ready → model-loaded → chat-ready).
  _waitForType(type) {
    return new Promise((resolve, reject) => {
      const handler = (e) => {
        if (e.data.type === type) {
          this._worker.removeEventListener('message', handler);
          resolve(e.data);
        } else if (e.data.type === 'error') {
          this._worker.removeEventListener('message', handler);
          reject(new Error(e.data.message));
        }
      };
      this._worker.addEventListener('message', handler);
    });
  }

  async _init({ modelUrl, modelBytes, onDownloadProgress, chatOptions }) {
    await this._ready;
    const bytes = modelBytes ?? await fetchModelBytes(modelUrl, onDownloadProgress);
    // Transfer the underlying buffer (zero-copy) into the worker.
    const wait = this._waitForType('model-loaded');
    this._worker.postMessage({ type: 'load-model', bytes }, [bytes.buffer]);
    await wait;
    const waitChat = this._waitForType('chat-ready');
    this._worker.postMessage({ type: 'create-chat', options: chatOptions });
    await waitChat;
  }

  // Steady-state message dispatch (post-init). Init replies are caught by
  // their own one-shot listeners and don't hit these cases.
  _onMessage(data) {
    switch (data.type) {
      case 'token':
        this._currentStream?._onToken(data.token);
        break;
      case 'ask-done': {
        const stream = this._currentStream;
        this._currentStream = null;
        stream?._onDone();
        break;
      }
      case 'error': {
        const stream = this._currentStream;
        this._currentStream = null;
        stream?._onError(new Error(data.message));
        break;
      }
    }
  }

  _onWorkerError(e) {
    const err = new Error(`worker crashed: ${e.message || 'unknown'}`);
    const stream = this._currentStream;
    this._currentStream = null;
    stream?._onError(err);
  }

  // Send a prompt and return a stream. Returns synchronously (not a Promise)
  // so callers can chain `.completed()` or `for await` directly, matching
  // Python's `chat.ask("...").completed()` shape.
  //
  // Only one ask can be in flight at a time — the underlying `askStreaming`
  // hook is per-thread and would misroute tokens otherwise (see core::llm's
  // set_streaming_hook docstring). Throws synchronously if violated.
  ask(prompt) {
    if (this._terminated) throw new Error('Chat: already terminated');
    if (this._currentStream) {
      throw new Error(
        'Chat.ask: another ask is in progress; await its .completed() or finish iterating first',
      );
    }
    const stream = new WorkerTokenStream();
    this._currentStream = stream;
    this._worker.postMessage({ type: 'ask', prompt });
    return stream;
  }

  // Shut down the worker. Any in-flight stream is rejected. After this the
  // instance is unusable.
  terminate() {
    if (this._terminated) return;
    this._terminated = true;
    const stream = this._currentStream;
    this._currentStream = null;
    stream?._onError(new Error('Chat terminated'));
    this._worker.terminate();
  }
}

// When this module is loaded inside a Web Worker (rather than the main
// thread), hand the message loop over to Rust. The Rust `runInWorker`
// export in js/src/lib.rs sets `self.onmessage` to handle the
// load-model / create-chat / ask protocol; before this hand-off existed,
// the dispatcher lived in worker.js as ~50 lines of JS.
//
// The main thread skips this branch (it's a Window, not a Worker scope),
// so app code that imports `Chat` from this module works the same.
if (typeof DedicatedWorkerGlobalScope !== 'undefined'
    && self instanceof DedicatedWorkerGlobalScope) {
  bg.runInWorker();
}
