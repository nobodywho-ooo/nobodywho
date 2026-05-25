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

// Emscripten's default `abort()` handler in a Node host calls
// `process.exit(1)` — bypassing both `process.on('uncaughtException')`
// and `process.on('exit')`-emitting-error paths. Override `onAbort` so
// abort messages reach stderr first, and override `quit` so the worker
// throws (which then triggers the uncaughtException handler set above
// in the worker's bootstrap preamble — see the Node spawn helper).
Module.onAbort = (what) => {
  try { process.stderr.write('[wasm abort] ' + (what === undefined ? '(no message)' : String(what)) + '\n'); }
  catch (e) { /* ignore */ }
  throw new Error('wasm aborted: ' + (what === undefined ? '(no message)' : String(what)));
};
Module.quit = (status, toThrow) => {
  // Emscripten calls quit() during abort and during natural exit. We
  // never want the worker to silently process.exit() — that hides
  // wasm traps and Rust panics from the parent. Always throw.
  throw toThrow || new Error('wasm quit(' + status + ')');
};

// Self-init: avoid making callers wire up panic hooks, the bootstrap URL,
// or worker-side message dispatch by hand. The Emscripten loader closure
// has `_scriptName = import.meta.url` in scope when `postRun` fires, so
// it can tell Rust where this loader lives — that's the URL the inline
// Blob worker in `Chat.create` re-imports.
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
  // Make `for await (const tok of chat.ask(prompt))` work. wasm-bindgen
  // 0.2.121 emits `next()` returning `Promise<{value, done}>` on the
  // TokenStream class but can't emit `[Symbol.asyncIterator]` cleanly,
  // so we attach the canonical "iterator returns itself" shim on the
  // prototype. One line of glue, unlocks the idiomatic JS streaming
  // pattern that the docs already assume.
  if (typeof Module.TokenStream === 'function'
      && Module.TokenStream.prototype
      && !Module.TokenStream.prototype[Symbol.asyncIterator]) {
    Module.TokenStream.prototype[Symbol.asyncIterator] = function () { return this; };
  }
  // Node-only: chunked host-fs → MEMFS streamer. Used by the worker
  // when Chat.create({modelPath}) is called — the worker streams the
  // model file into MEMFS in 64 MiB chunks without holding the full
  // bytes in JS memory, bypassing Node's 2 GiB fs.readFileSync cap.
  // Note: doesn't help wasm32's 4 GiB linear-memory ceiling — model
  // tensors still have to fit in wasm memory once loaded.
  if (typeof process !== 'undefined' && process.versions && process.versions.node) {
    // Async sync-style read for Audio.fromPath / Image.fromPath. Returns
    // a Uint8Array. Node-only — the helper isn't defined in browser, so
    // the Rust side's lookup fails clearly there.
    globalThis.__nbw_node_read_file = async function (srcPath) {
      const fs = await import('node:fs');
      return new Uint8Array(fs.readFileSync(srcPath));
    };

    globalThis.__nbw_node_file_to_memfs = async function (srcPath, memfsPath) {
      const fs = await import('node:fs');
      const size = fs.statSync(srcPath).size;
      const CHUNK = 64 * 1024 * 1024;
      const srcFd = fs.openSync(srcPath, 'r');
      const memfsStream = FS.open(memfsPath, 'w');
      try {
        let offset = 0;
        while (offset < size) {
          const toRead = Math.min(CHUNK, size - offset);
          // Allocate a fresh Uint8Array per chunk and read the slice
          // into its own backing storage. Buffer.allocUnsafe + FS.write
          // with the (offset,length,position) form had FS.write writing
          // wrong bytes (bad GGUF magic in the resulting MEMFS file) —
          // suspected interaction between Node Buffer view + FS.write's
          // offset/length handling. A fresh exactly-sized Uint8Array
          // per chunk avoids any aliasing confusion at FS.write time.
          const chunkBuf = new Uint8Array(toRead);
          const bytesRead = fs.readSync(srcFd, chunkBuf, 0, toRead, offset);
          if (bytesRead === 0) {
            throw new Error(`__nbw_node_file_to_memfs: unexpected EOF at ${offset} (expected ${size})`);
          }
          const written = FS.write(memfsStream, chunkBuf, 0, bytesRead);
          if (written !== bytesRead) {
            throw new Error(`__nbw_node_file_to_memfs: short write (${written}/${bytesRead}) at offset ${offset}`);
          }
          offset += bytesRead;
        }
      } finally {
        fs.closeSync(srcFd);
        FS.close(memfsStream);
      }
      return memfsPath;
    };
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
      `import { parentPort, receiveMessageOnPort } from 'node:worker_threads';`,
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
      // Sync-drain pending parentPort messages from inside the wasm
      // inference loop. Needed for Chat.stopGeneration() to take
      // effect mid-ask: the worker is busy running wasm and won't
      // dispatch onmessage until the call returns, so the 'stop'
      // message sits in the queue. Calling this from the per-token
      // hook lets the existing onmessage handler fire between
      // tokens, which routes 'stop' to ChatHandleAsync::stop_generation
      // and the inference loop breaks on the next token.
      // Node-only — Web Workers have no synchronous message
      // receive primitive. Browser stop only takes effect after the
      // current ask completes (or via Chat.terminate() to nuke).
      `globalThis.__nbw_drain_messages = () => {`,
      `  let m;`,
      `  while ((m = receiveMessageOnPort(parentPort)) !== undefined) {`,
      `    const data = m.message;`,
      `    // Special-case 'stop': call the sync wasm export directly so the`,
      `    // inference loop sees the flag on the next token. Going through`,
      `    // __nbw_onmessage (which schedules an async dispatcher via`,
      `    // spawn_local) wouldn't work mid-ask because spawn_local-queued`,
      `    // futures can't run while the wasm event loop is blocked.`,
      `    if (data && data.type === 'stop' && typeof Module !== 'undefined' && typeof Module.stopCurrentAsk === 'function') {`,
      `      Module.stopCurrentAsk();`,
      `      continue;`,
      `    }`,
      `    if (__nbw_onmessage) __nbw_onmessage({ data });`,
      `  }`,
      `};`,
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
      // Catch process.exit() invocations so the parent sees WHY the
      // worker exited. Emscripten's default abort handler in Node
      // calls process.exit(1) silently. The 'exit' event fires even
      // for explicit exit(); 'beforeExit' fires when the event loop
      // empties naturally. Both log to stderr so the parent's pipe
      // captures the signal.
      `process.on('exit', (code) => {`,
      `  process.stderr.write('[worker exit] code=' + code + ' stack=' + (new Error().stack || '') + '\\n');`,
      `});`,
      `process.on('beforeExit', (code) => {`,
      `  process.stderr.write('[worker beforeExit] code=' + code + '\\n');`,
      `});`,
      // Wrap process.exit so we capture WHO called it. The stack
      // trace in the wrapped call tells us if it was Emscripten's
      // abort path, a user-side process.exit, or some library.
      `const __real_exit = process.exit;`,
      `process.exit = function(code) {`,
      `  process.stderr.write('[worker process.exit called] code=' + code + ' stack=' + (new Error().stack || '') + '\\n');`,
      `  return __real_exit.call(process, code);`,
      `};`,
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
