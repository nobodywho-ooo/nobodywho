// Node bootstrap for the wasm bundle. Importing this module loads the wasm
// from ../pkg-bundler/, wires up WASI + wasm-bindgen, and re-exports the
// binding classes. Demo files just say:
//
//   import { Model, Chat } from './setup.mjs';
//
// and write code that looks like the Python examples next door. The
// `nobodywho-js` npm package will eventually do this inside its own entry
// point — at that point the demos collapse to a single `import 'nobodywho-js'`,
// matching `import nobodywho` in Python.
//
// For the browser case the bootstrap is different (uses
// `@bjorn3/browser_wasi_shim` instead of `node:wasi`). See examples/browser*.html.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { WASI } from 'node:wasi';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const bg = await import(join(pkgDir, 'nobodywho_js_bg.js'));

// Empty preopens = no filesystem visibility from inside wasm, matching the
// browser sandbox.
const wasi = new WASI({ version: 'preview1', args: [], env: {} });

// Imports we don't actually exercise: mtmd_* (multimodal C++ skipped from
// the wasi-libc build), _Unwind_* (legacy exception ABI), dlclose. Throw on
// call so accidental use shows up loud during development.
const envStubs = new Proxy({}, {
  get: (_t, name) => (...args) => {
    throw new Error(`unresolved env.${String(name)}(${args.join(', ')})`);
  },
});

const wasmBytes = readFileSync(join(pkgDir, 'nobodywho_js_bg.wasm'));
const inst = await WebAssembly.instantiate(await WebAssembly.compile(wasmBytes), {
  './nobodywho_js_bg.js': bg,
  env: envStubs,
  ...wasi.getImportObject(),
});

// `wasi.initialize` runs `_initialize`, which covers libc + libc++ static
// ctors. Then `__wbindgen_start` does wasm-bindgen's own startup (externref
// table etc.). Each must run exactly once.
wasi.initialize(inst);
bg.__wbg_set_wasm(inst.exports);
if (inst.exports.__wbindgen_start) inst.exports.__wbindgen_start();

// Installs the panic hook + tracing subscriber. Idempotent.
bg.init();

export const { Model, Chat, Encoder, TokenStream } = bg;
