// Web Worker: runs the wasm + llama.cpp off the main thread.
//
// All wasm calls happen here; the main page only posts messages and
// receives results. Messages are tiny (text in, text out) but the model
// bytes get transferred (zero-copy) via postMessage's `transfer` arg.

import { WASI, OpenFile, File, ConsoleStdout } from
  'https://esm.sh/@bjorn3/browser_wasi_shim@0.4.1';

const send = (type, extra = {}) => self.postMessage({ type, ...extra });

// Resolved during the first 'init' message. Held in module scope so
// subsequent messages can reuse the loaded wasm + model + chat.
let bg;
let model;
let chat;

const envStubs = new Proxy({}, {
  get: (_t, name) => (...args) => {
    throw new Error(`unresolved env.${String(name)}(${args.join(', ')})`);
  },
});

async function init() {
  send('progress', { stage: 'fetching-wasm' });
  const [wasmBytes, bgModule] = await Promise.all([
    fetch(new URL('../pkg-bundler/nobodywho_js_bg.wasm', import.meta.url))
      .then((r) => r.arrayBuffer()),
    import(new URL('../pkg-bundler/nobodywho_js_bg.js', import.meta.url).href),
  ]);
  bg = bgModule;

  send('progress', { stage: 'instantiating', wasmBytes: wasmBytes.byteLength });
  const wasi = new WASI([], [], [
    new OpenFile(new File([])),
    ConsoleStdout.lineBuffered(() => {}),
    ConsoleStdout.lineBuffered(() => {}),
  ]);
  const wmod = await WebAssembly.compile(wasmBytes);
  const inst = await WebAssembly.instantiate(wmod, {
    './nobodywho_js_bg.js': bg,
    env: envStubs,
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  wasi.initialize(inst);
  bg.__wbg_set_wasm(inst.exports);
  if (inst.exports.__wbindgen_start) inst.exports.__wbindgen_start();
  bg.init();

  send('ready');
}

async function loadModel(bytes) {
  send('progress', { stage: 'loading-model', modelBytes: bytes.byteLength });
  model = await bg.Model.loadBytes(bytes);
  send('model-loaded');
}

function createChat(options) {
  chat = new bg.Chat(model, options);
  send('chat-ready');
}

async function ask(prompt) {
  // Use `askStreaming` (not `ask` + `nextToken()`) so the Rust inference loop
  // calls back into JS per token directly. From a Web Worker, the JS callback
  // posts to the main thread, which sees streaming in real time. The
  // `ask` + `nextToken()` path batches everything until inference finishes
  // because sync wasm doesn't yield to the event loop between tokens.
  const t0 = performance.now();
  let count = 0;
  await chat.askStreaming(prompt, (token) => {
    send('token', { token });
    count++;
  });
  const dt = (performance.now() - t0) / 1000;
  send('ask-done', { count, seconds: dt });
}

self.onmessage = async (e) => {
  const { type, ...rest } = e.data;
  try {
    switch (type) {
      case 'init':         await init(); break;
      case 'load-model':   await loadModel(rest.bytes); break;
      case 'create-chat':  createChat(rest.options); break;
      case 'ask':          await ask(rest.prompt); break;
      default: send('error', { message: `unknown message type: ${type}` });
    }
  } catch (err) {
    send('error', { message: String(err), stack: err?.stack });
  }
};
