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
// each wasm instance lives in its own JS realm, so the global assignment
// is unambiguous within that realm.
globalThis.Module = Module;

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
