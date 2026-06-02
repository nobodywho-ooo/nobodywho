// wasm-bindgen's Emscripten output references a `HEAP_DATA_VIEW`
// global it never declares. Define it as a lazy getter that refreshes
// when memory grows (HEAPU8.buffer identity change).
//
// This runs at pre-js top level (in the module factory), NOT in
// Module.preRun, so it executes on EVERY thread — including pthread
// workers. preRun runs on the main thread only, but pthread workers also
// run the factory; they need their own HEAP_DATA_VIEW because any
// wasm-bindgen JS-interop shim invoked there reads it. Concretely, the
// inference worker runs on a pthread and emits `tracing` logs, and the
// global tracing-wasm subscriber timestamps them via `performance.now()`,
// which compiles to `__wbindgen_number_get` → HEAP_DATA_VIEW. The value
// is produced and consumed on the same worker, so a per-thread DataView
// over that thread's HEAPU8 is correct. The getter is lazy, so HEAPU8
// just needs to be assigned before the first access (always true once the
// runtime is up).
{
  let buf = null, view = null;
  Object.defineProperty(globalThis, 'HEAP_DATA_VIEW', {
    configurable: true,
    get() {
      if (buf !== HEAPU8.buffer) { buf = HEAPU8.buffer; view = new DataView(buf); }
      return view;
    },
  });
}
Module.preRun = Module.preRun || [];

// Expose this instance's Module on globalThis so Rust code inside the
// wasm can reach Module.FS.writeFile via js_sys::Reflect lookups (used
// by Path A's MEMFS write helpers in src/lib.rs). MODULARIZE=1 keeps
// Module local to the factory closure by default; for our purposes
// each wasm instance lives in its own JS realm, so the global assignment
// is unambiguous within that realm.
globalThis.Module = Module;

// Reference-counted event-loop keepalive. Inference runs on an Emscripten
// pthread (Web Worker); tokens come back to the main thread via a tokio
// channel whose waker posts cross-thread. In Node, a top-level-await that
// never yields to a macrotask can leave the event loop without pumping the
// pthread message ports, so the cross-thread wake never gets delivered and
// the first `ask`/`encode` deadlocks. A ref'd timer ticking every 50ms
// keeps the loop pumping. It's ref-counted (acquired around each async op
// via the Rust `promisify` helper, released on completion) so it only runs
// while inference is in flight and never blocks process exit when idle.
if (!globalThis.__nbw_keepalive_acquire) {
  let count = 0, timer = null;
  globalThis.__nbw_keepalive_acquire = () => {
    count++;
    if (!timer) timer = setInterval(() => {}, 50);
  };
  globalThis.__nbw_keepalive_release = () => {
    if (--count <= 0) {
      count = 0;
      if (timer) { clearInterval(timer); timer = null; }
    }
  };
}

// Skip Emscripten's MEMFS permission checks at the libc syscall layer.
Module.preRun.push(() => {
  FS.ignorePermissions = true;
});

// Capture wasm stderr (Module.printErr) explicitly.
Module.printErr = (line) => {
  try { process.stderr.write('[wasm stderr] ' + line + '\n'); }
  catch (e) { console.error('[wasm stderr]', line); }
};

Module.onAbort = (what) => {
  try { process.stderr.write('[wasm abort] ' + (what === undefined ? '(no message)' : String(what)) + '\n'); }
  catch (e) { /* ignore */ }
  throw new Error('wasm aborted: ' + (what === undefined ? '(no message)' : String(what)));
};
Module.quit = (status, toThrow) => {
  throw toThrow || new Error('wasm quit(' + status + ')');
};

Module.postRun = Module.postRun || [];
Module.postRun.push(() => {
  if (typeof Module.init === 'function') Module.init();
  // Make `for await (const tok of chat.ask(prompt))` work.
  if (typeof Module.TokenStream === 'function'
      && Module.TokenStream.prototype
      && !Module.TokenStream.prototype[Symbol.asyncIterator]) {
    Module.TokenStream.prototype[Symbol.asyncIterator] = function () { return this; };
  }

  // Single-copy model load (browser + Node): wrap a buffer that already
  // lives in wasm linear memory (Rust streamed the model into it) as a
  // MEMFS file, WITHOUT copying. `node.contents` is a getter returning a
  // fresh Uint8Array view over the *current* wasm memory at [ptr, ptr+len)
  // — fresh each access so it survives ALLOW_MEMORY_GROWTH detaching the
  // old ArrayBuffer. Since the view's .buffer === HEAP.buffer and llama.cpp
  // mmaps with MAP_SHARED, MEMFS.mmap returns it zero-copy
  // (contents.byteOffset) instead of allocate+copy; fread/seek/stat read
  // straight from the view too.
  globalThis.__nbw_wrap_wasm_buffer_as_file = function (path, ptr, len) {
    const parts = path.split('/').filter(Boolean);
    const fname = parts.pop();
    let cur = '';
    for (const p of parts) { cur += '/' + p; try { FS.mkdir(cur); } catch (e) { /* EEXIST */ } }
    try { FS.unlink(path); } catch (e) { /* ENOENT */ }
    const node = FS.create('/' + parts.join('/') + '/' + fname, 0o444);
    Object.defineProperty(node, 'contents', {
      configurable: true,
      get() {
        const buf = (typeof wasmMemory !== 'undefined' && wasmMemory.buffer) || HEAPU8.buffer;
        return new Uint8Array(buf, ptr, len);
      },
      set() { /* read-only model file; ignore writes */ },
    });
    node.usedBytes = len;
    return path;
  };

  if (typeof process !== 'undefined' && process.versions && process.versions.node) {
    globalThis.__nbw_node_read_file = async function (srcPath) {
      const fs = await import('node:fs');
      return new Uint8Array(fs.readFileSync(srcPath));
    };

    globalThis.__nbw_mount_nodefs = function (hostDir, mountpoint) {
      const parts = mountpoint.split('/').filter(Boolean);
      let cur = '';
      for (const p of parts) {
        cur += '/' + p;
        try { FS.mkdir(cur); } catch (e) { /* EEXIST is fine */ }
      }
      try {
        FS.mount(NODEFS, { root: hostDir }, mountpoint);
      } catch (e) {
        if (e.errno !== 10) throw e; // 10 = EBUSY → already mounted
      }
      return mountpoint;
    };
  }
});
