// Path A end-to-end vision smoke test.
//
// Loads Qwen2.5-Omni-3B (~2 GB main model + ~1.5 GB mmproj) via
// `Model.loadBytes(modelBytes, mmprojBytes)` — the new Path A API that
// writes the mmproj bytes into Emscripten's MEMFS and loads through the
// existing path-based ProjectionModel. Then asks the model to identify
// a penguin image passed through `Image.fromBytes(uint8)` (same MEMFS-
// backed mechanism).
//
// What's being validated end-to-end:
//   * Model.loadBytes accepts an optional mmproj-bytes second argument
//     and produces a Model with a working projection_model.
//   * Image.fromBytes wraps raw bytes in a tagged object whose worker-
//     side handler writes to /tmp/nbw-image-<hash>.bin in MEMFS and
//     pushes the resulting path through Prompt::push_image.
//   * chat.ask(['describe this', img]) runs the full multimodal
//     inference loop and returns a sensible description.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/vision-smoke.mjs
//
// Defaults to Qwen-Omni-3B at /tmp/qwen-omni/* and the penguin.png from
// the Python tests. Override with positional args.

import { readFileSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(here, '..', 'pkg-bundler');

const modelPath = process.argv[2] ?? '/tmp/qwen-omni/Qwen2.5-Omni-3B-Q4_K_M.gguf';
const mmprojPath = process.argv[3] ?? '/tmp/qwen-omni/mmproj-Qwen2.5-Omni-3B-Q8_0.gguf';
const imagePath = process.argv[4]
  ?? resolve(here, '..', '..', 'python', 'tests', 'img', 'penguin.png');

for (const p of [modelPath, mmprojPath, imagePath]) {
  if (!existsSync(p)) { console.error('missing required file:', p); process.exit(2); }
}

console.log('Loading wasm module...');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();
console.log('  module loaded.');

// Sanity: Image factory is exposed.
assert.equal(typeof m.Image, 'function', 'expected m.Image factory');
assert.equal(typeof m.Image.fromBytes, 'function', 'expected m.Image.fromBytes');

console.log('\nReading model + mmproj bytes...');
const modelBytes = new Uint8Array(readFileSync(modelPath));
const mmprojBytes = new Uint8Array(readFileSync(mmprojPath));
const imgBytes = new Uint8Array(readFileSync(imagePath));
console.log(`  main model: ${(modelBytes.byteLength / 1e6).toFixed(0)} MB`);
console.log(`  mmproj:     ${(mmprojBytes.byteLength / 1e6).toFixed(0)} MB`);
console.log(`  image:      ${imgBytes.byteLength} bytes`);

console.log('\nLoading multimodal model (Model.loadBytes with mmproj)...');
const t0 = performance.now();
const model = await m.Model.loadBytes(modelBytes, mmprojBytes);
console.log(`  loaded in ${(performance.now() - t0).toFixed(0)} ms`);

console.log('\nBuilding Chat (n_ctx=16384 to fit one image)...');
const chat = new m.Chat(model, {
  systemPrompt: 'You are a helpful assistant. Be brief.',
  templateVariables: { enable_thinking: false },
  nCtx: 16384,
});

console.log('\nCalling chat.ask([text, Image.fromBytes(uint8)])...');
const tAsk = performance.now();
const img = m.Image.fromBytes(imgBytes);
console.log(`  Image part shape: __nbwKind=${img.__nbwKind}, ${img.bytes.byteLength} bytes`);

const stream = await chat.ask([
  'What animal is in this image? One word, lowercase.',
  img,
]);
const response = await stream.completed();
const dt = (performance.now() - tAsk) / 1000;
console.log(`\n=== Response (${dt.toFixed(1)} s) ===`);
console.log(response);

const containsPenguin = response.toLowerCase().includes('penguin');
console.log(`\ncontains "penguin": ${containsPenguin}`);
if (!containsPenguin) {
  console.error('FAIL: model did not identify a penguin');
  process.exit(1);
}

console.log('\n=== Path A end-to-end vision smoke passed ===');
console.log('  Model.loadBytes(modelBytes, mmprojBytes) wires mmproj through MEMFS,');
console.log('  Image.fromBytes(uint8) wires image bytes through MEMFS,');
console.log('  multimodal inference returns a sensible answer.');
