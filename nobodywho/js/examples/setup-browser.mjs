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

// Re-export everything from the wasm. `Chat` is the worker-backed Rust
// class users actually call; `ChatRaw` is the raw blocking class kept for
// advanced use cases (e.g. when you've already arranged your own
// off-main-thread context).
export const { Model, Chat, ChatRaw, Encoder, CrossEncoder, TokenStream,
               WorkerTokenStream, fetchModelBytes } = bg;

// wasm-bindgen 0.2.121 can't emit Symbol-keyed exports cleanly, so the
// async-iterator protocol is added at the prototype level here. Lets
// `for await (const tok of chat.ask(prompt))` work alongside the explicit
// `await stream.completed()` shape.
bg.WorkerTokenStream.prototype[Symbol.asyncIterator] = function () { return this; };

// Tell the Rust side where setup-browser.mjs lives, so `Chat.create()` can
// build the inline-Blob worker bootstrap that re-imports this module from
// inside the worker context.
bg.setBootstrapUrl(import.meta.url);

// When this module is loaded inside a Web Worker (rather than the main
// thread), hand the message loop over to Rust's `runInWorker` (the worker
// side of the load-model / create-chat / ask protocol that Chat speaks).
if (typeof DedicatedWorkerGlobalScope !== 'undefined'
    && self instanceof DedicatedWorkerGlobalScope) {
  bg.runInWorker();
}
