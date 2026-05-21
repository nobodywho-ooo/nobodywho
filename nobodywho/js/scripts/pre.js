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

// Self-init: avoid making callers wire up panic hooks, the bootstrap URL,
// or worker-side message dispatch by hand. The Emscripten loader closure
// has `_scriptName = import.meta.url` in scope when `postRun` fires, so
// it can tell Rust where this loader lives — that's the URL the inline
// Blob worker in `WorkerChat.create` re-imports.
//
// In a Web Worker context the same hook also takes over `self.onmessage`
// via `runInWorker()`, so the JS host doesn't have to call it explicitly
// from a separate setup module.
Module.postRun = Module.postRun || [];
Module.postRun.push(() => {
  if (typeof Module.init === 'function') Module.init();
  if (typeof Module.setBootstrapUrl === 'function' && typeof _scriptName === 'string') {
    Module.setBootstrapUrl(_scriptName);
  }
  if (typeof DedicatedWorkerGlobalScope !== 'undefined'
      && self instanceof DedicatedWorkerGlobalScope
      && typeof Module.runInWorker === 'function') {
    Module.runInWorker();
  }
});
