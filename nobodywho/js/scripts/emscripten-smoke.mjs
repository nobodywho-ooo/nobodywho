// Phase 0 verification smoke test for the wasm32-unknown-emscripten port.
//
// Loads the Emscripten-emitted nobodywho_js.js (which auto-instantiates the
// wasm and runs wasm-bindgen's start function via the modularized factory),
// then exercises Model.load + Encoder.encode end-to-end on a tiny
// embedding model. Passes iff a finite Float32Array of the expected
// dimension comes back.
//
// Run after building with the Emscripten toolchain:
//
//   EMSDK=... cargo build --target wasm32-unknown-emscripten --release -p nobodywho-js
//   node js/scripts/emscripten-smoke.mjs /path/to/bge-small.gguf
//
// Exit 0 = both Rust side and llama.cpp side work end-to-end under
// Emscripten + pthreads. Exit non-zero = phase 0 hasn't passed yet; see the
// printed error.

import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = fileURLToPath(new URL('.', import.meta.url));
const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: node emscripten-smoke.mjs <path-to-embedding.gguf>');
  console.error('  tip: download bge-small-en-v1.5-q8_0.gguf from HuggingFace');
  process.exit(2);
}
if (!existsSync(modelPath)) {
  console.error(`model not found: ${modelPath}`);
  process.exit(2);
}

// The Emscripten loader lives in pkg-bundler/ after a successful build
// (build-pkg.sh post-Phase-2). During Phase 0 we point directly at the
// raw cargo output instead.
const candidates = [
  resolve(here, '..', 'pkg-bundler', 'nobodywho_js.js'),
  resolve(
    here,
    '..',
    '..',
    'target',
    'wasm32-unknown-emscripten',
    'release',
    'nobodywho_js.js',
  ),
  resolve(
    here,
    '..',
    '..',
    'target',
    'wasm32-unknown-emscripten',
    'debug',
    'nobodywho_js.js',
  ),
];
const loaderPath = candidates.find(existsSync);
if (!loaderPath) {
  console.error('Emscripten loader not found. Looked at:');
  for (const c of candidates) console.error('  ' + c);
  console.error('Build first: cargo build --target wasm32-unknown-emscripten -p nobodywho-js');
  process.exit(2);
}
console.log(`loading: ${loaderPath}`);

// `createNobodyWhoModule` is the factory function exported by the
// MODULARIZE=1 + EXPORT_NAME='createNobodyWhoModule' link flags in
// js/build.rs. It returns a Promise that resolves to the Module object
// with all wasm-bindgen exports attached as properties.
const { default: createNobodyWhoModule } = await import(loaderPath);
const module = await createNobodyWhoModule({
  // Help the loader find sibling .wasm / .data files (Emscripten ships
  // multiple .wasm modules when pthreads are enabled — the main wasm plus
  // a "pthread worker" .js / .wasm pair).
  locateFile: (path) => resolve(loaderPath, '..', path),
});

// wasm-bindgen exports are attached as Module properties. Sanity-check the
// shape before invoking any class.
for (const sym of ['Model', 'Encoder', 'init']) {
  if (typeof module[sym] !== 'function') {
    console.error(`expected ${sym} to be a function on the module, got ${typeof module[sym]}`);
    console.error('exports:', Object.keys(module).filter((k) => !k.startsWith('_')).slice(0, 30));
    process.exit(1);
  }
}

console.log('Model.load({modelPath})...');
const model = await module.Model.load({ modelPath });

console.log('new Encoder + encode("test")...');
const encoder = new module.Encoder(model, 2048);
const vec = await encoder.encode('test');

console.log(`embedding dimension: ${vec.length}`);
assert.equal(typeof vec.length, 'number', 'expected typed-array shape');
assert.ok(vec.length > 0, 'embedding is empty');
assert.ok(
  Array.from(vec.slice(0, 8)).every((x) => Number.isFinite(x)),
  `first 8 values must be finite, got ${Array.from(vec.slice(0, 8))}`,
);
console.log(`first 8: [${Array.from(vec.slice(0, 8)).map((x) => x.toFixed(4)).join(', ')}]`);

console.log('\nOK — Emscripten + wasm-bindgen + llama.cpp inference path verified.');
