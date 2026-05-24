// wasm-bindgen 0.2.121 Emscripten output references a `HEAP_DATA_VIEW`
// global it never declares. Define it here as a lazy getter that
// refreshes when memory grows (HEAPU8.buffer identity change).
Module.preRun = Module.preRun || [];
Module.preRun.push(() => {
  let buf = null, view = null;
  Object.defineProperty(globalThis, 'HEAP_DATA_VIEW', {
    configurable: true,
    get() {
      if (buf !== HEAPU8.buffer) { buf = HEAPU8.buffer; view = new DataView(buf); }
      return view;
    },
  });
});

// Expose this instance's Module on globalThis so Rust code inside the
// wasm can reach Module.FS.writeFile via js_sys::Reflect lookups (used
// by Path A's MEMFS write helpers in src/lib.rs). MODULARIZE=1 keeps
// Module local to the factory closure by default; for our purposes
// each wasm instance lives in its own JS realm (main thread + each
// Web Worker), so the global assignment is unambiguous within that
// realm. Multiple loaders in the same realm would clobber each other,
// but that's not a configuration we support.
globalThis.Module = Module;

// Skip Emscripten's MEMFS permission checks at the libc syscall layer.
// Without this, files written via `Module.FS.writeFile` (which run as
// the implicit "root" JS context) are mode-100666 and uid/gid 0, but
// libc `open(2)` from inside the wasm runs with a non-root effective
// uid, fails Emscripten's check_access, and returns EPERM (errno 63).
// We need both the JS-side write *and* the libc-side read to work
// against the same MEMFS path — Path A uses JS for the write and
// stb_image / miniaudio / gguf_init_from_file (all libc fopen) for
// the read. `FS.ignorePermissions = true` tells the MEMFS layer to
// stop enforcing permissions; the wasm runs in a single-user sandbox
// so the check has no real security meaning here anyway.
Module.preRun.push(() => {
  FS.ignorePermissions = true;
});

// Capture wasm stderr (Module.printErr) explicitly. Otherwise C-level
// abort() messages and ggml LOG_ERR output are written to console.error
// which in Node worker_threads doesn't always make it back to the main
// stderr stream with full content. We tee to process.stderr so panics
// and aborts are visible in test logs.
Module.printErr = (line) => {
  try { process.stderr.write('[wasm stderr] ' + line + '\n'); }
  catch (e) { console.error('[wasm stderr]', line); }
};

// Self-init: avoid making callers wire up panic hooks, the bootstrap URL,
// or worker-side message dispatch by hand. The Emscripten loader closure
// has `_scriptName = import.meta.url` in scope when `postRun` fires, so
// it can tell Rust where this loader lives — that's the URL the inline
// Blob worker in `WorkerChat.create` re-imports.
//
// Auto-runs `runInWorker()` in any worker context — browser
// `DedicatedWorkerGlobalScope` OR a Node `worker_threads` worker whose
// bootstrap polyfilled `globalThis.__nbw_node_worker = true` before
// importing this module.
Module.postRun = Module.postRun || [];
Module.postRun.push(() => {
  if (typeof Module.init === 'function') Module.init();
  if (typeof Module.setBootstrapUrl === 'function' && typeof _scriptName === 'string') {
    Module.setBootstrapUrl(_scriptName);
  }
  const isBrowserWorker = typeof DedicatedWorkerGlobalScope !== 'undefined'
    && typeof self !== 'undefined'
    && self instanceof DedicatedWorkerGlobalScope;
  const isNodeWorker = globalThis.__nbw_node_worker === true;
  if ((isBrowserWorker || isNodeWorker) && typeof Module.runInWorker === 'function') {
    Module.runInWorker();
  }
});

// JS-side worker spawn helper, exposed to Rust as `globalThis.__nbw_spawn_worker`.
// Returns a Promise<Worker-like> in both environments. The Rust side calls this
// instead of constructing a `Worker` directly so the choice between the browser
// Web Worker API and Node's `worker_threads` happens here in one place.
//
// The returned object always has the SHAPE of a browser Worker:
//   postMessage(msg), terminate(), onmessage (setter), onerror (setter)
// In Node, that shape is provided by a small shim wrapping `worker_threads.Worker`
// so the Rust main-side code can treat both uniformly via Reflect.
globalThis.__nbw_spawn_worker = async function(wasmEntryUrl) {
  // Browser: blob URL + new Worker. Same path that has worked all along —
  // just routed through this helper for parity.
  if (typeof Worker !== 'undefined' && typeof Blob !== 'undefined'
      && typeof URL !== 'undefined' && typeof URL.createObjectURL === 'function') {
    const bootstrap =
      `import(${JSON.stringify(wasmEntryUrl)}).then(({default:c}) => c());`;
    const blob = new Blob([bootstrap], { type: 'text/javascript' });
    const url = URL.createObjectURL(blob);
    return new Worker(url, { type: 'module' });
  }

  // Node: worker_threads + data URL with a polyfill preamble. The polyfill
  // shims globalThis.postMessage / globalThis.onmessage from parentPort so
  // the existing worker-side Rust code (which now uses Reflect on
  // globalThis) works unchanged. `globalThis.__nbw_node_worker = true`
  // tells pre.js (loaded inside the spawned worker) to fire runInWorker().
  if (typeof process !== 'undefined' && process.versions && process.versions.node) {
    const { Worker: NodeWorker } = await import('node:worker_threads');
    const preamble = [
      `import { parentPort } from 'node:worker_threads';`,
      `globalThis.__nbw_node_worker = true;`,
      `globalThis.postMessage = (m) => parentPort.postMessage(m);`,
      `let __nbw_onmessage;`,
      `Object.defineProperty(globalThis, 'onmessage', {`,
      `  configurable: true,`,
      `  get() { return __nbw_onmessage; },`,
      `  set(fn) {`,
      `    __nbw_onmessage = fn;`,
      `    if (!parentPort.__nbw_wired) {`,
      `      parentPort.on('message', (data) => __nbw_onmessage && __nbw_onmessage({ data }));`,
      `      parentPort.__nbw_wired = true;`,
      `    }`,
      `  },`,
      `});`,
      // Worker-side uncaughtException handler: prints the FULL error
      // (including .stack) to stderr before the worker dies. Without
      // this, main only sees an empty Error and "WebAssembly.Exception {}"
      // — no message, no stack — so wasm traps + C++ aborts are invisible.
      `process.on('uncaughtException', (err) => {`,
      `  process.stderr.write('[worker uncaughtException] ' + (err && err.stack ? err.stack : String(err)) + '\\n');`,
      `  if (err && typeof err === 'object') {`,
      `    for (const k of Object.getOwnPropertyNames(err)) {`,
      `      try { process.stderr.write('  err.' + k + ' = ' + JSON.stringify(err[k]) + '\\n'); } catch {}`,
      `    }`,
      `  }`,
      `  process.exit(1);`,
      `});`,
      `process.on('unhandledRejection', (reason) => {`,
      `  process.stderr.write('[worker unhandledRejection] ' + (reason && reason.stack ? reason.stack : String(reason)) + '\\n');`,
      `  process.exit(1);`,
      `});`,
      `import(${JSON.stringify(wasmEntryUrl)}).then(({default:c}) => c());`,
    ].join('\n');
    const dataUrl =
      'data:text/javascript;base64,' + Buffer.from(preamble).toString('base64');
    const nodeWorker = new NodeWorker(new URL(dataUrl), { type: 'module' });
    return __nbw_wrap_node_worker(nodeWorker);
  }

  throw new Error(
    '__nbw_spawn_worker: no Worker API available (need browser Worker or Node worker_threads)',
  );
};

function __nbw_wrap_node_worker(w) {
  // Shim a worker_threads.Worker into the browser Worker shape. The Rust
  // main-side code calls .postMessage / .terminate and assigns .onmessage /
  // .onerror via Reflect — these are the only methods/properties accessed.
  let _onmessage = null;
  let _onerror = null;
  w.on('message', (data) => {
    if (typeof _onmessage === 'function') _onmessage({ data });
  });
  w.on('error', (err) => {
    if (typeof _onerror === 'function') {
      _onerror({ message: (err && err.message) || String(err) });
    }
  });
  const shim = {
    postMessage: (m) => w.postMessage(m),
    terminate: () => { w.terminate(); },
  };
  Object.defineProperty(shim, 'onmessage', {
    configurable: true,
    get() { return _onmessage; },
    set(fn) { _onmessage = fn; },
  });
  Object.defineProperty(shim, 'onerror', {
    configurable: true,
    get() { return _onerror; },
    set(fn) { _onerror = fn; },
  });
  return shim;
}
