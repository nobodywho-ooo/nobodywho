// Verification test for the wasm64-unknown-emscripten (MEMORY64) port.
//
// The wasm64 sibling of test_emscripten.mjs: identical end-to-end check
// (Model.load + Encoder.encode on a tiny embedding model), but loads the
// loader from pkg-bundler-wasm64/ instead of pkg-bundler/. Passes iff a
// finite Float32Array of the expected dimension comes back — i.e. the
// Emscripten + wasm-bindgen + llama.cpp path works under a 64-bit-memory
// (MEMORY64) module.
//
// Run after building with build-pkg-emscripten-wasm64.sh:
//
//   bash js/scripts/build-pkg-emscripten-wasm64.sh
//   node js/tests/test_emscripten_wasm64.mjs /path/to/bge-small.gguf
//
// A passing run with a small model proves the 64-bit build is sound. The
// actual PAYOFF of wasm64 — loading a model whose working set exceeds the
// wasm32 4 GiB ceiling — is proven by running this same test against such a
// model (e.g. a Gemma 3 4B Q4_K_M + mmproj that OOMs on the wasm32 build).
//
// Exit 0 = the 64-bit Rust + llama.cpp inference path works end-to-end.
// Exit non-zero = see the printed error.

import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = fileURLToPath(new URL('.', import.meta.url));
const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: node test_emscripten_wasm64.mjs <path-to-embedding.gguf>');
  console.error('  tip: download bge-small-en-v1.5-q8_0.gguf from HuggingFace');
  process.exit(2);
}
if (!existsSync(modelPath)) {
  console.error(`model not found: ${modelPath}`);
  process.exit(2);
}

// The Emscripten loader lives in pkg-bundler-wasm64/ after a successful build
// (build-pkg-emscripten-wasm64.sh); fall back to the raw cargo output if that
// hasn't been run yet.
const candidates = [
  resolve(here, '..', 'pkg-bundler-wasm64', 'nobodywho_js.js'),
  resolve(
    here,
    '..',
    '..',
    'target',
    'wasm64-unknown-emscripten',
    'release',
    'nobodywho_js.js',
  ),
  resolve(
    here,
    '..',
    '..',
    'target',
    'wasm64-unknown-emscripten',
    'debug',
    'nobodywho_js.js',
  ),
];
const loaderPath = candidates.find(existsSync);
if (!loaderPath) {
  console.error('wasm64 Emscripten loader not found. Looked at:');
  for (const c of candidates) console.error('  ' + c);
  console.error('Build first: bash js/scripts/build-pkg-emscripten-wasm64.sh');
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

console.log('\nOK — wasm64 (MEMORY64) Emscripten + wasm-bindgen + llama.cpp path verified.');
