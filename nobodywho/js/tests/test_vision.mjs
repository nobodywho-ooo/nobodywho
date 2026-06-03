// Path A end-to-end vision test.
//
// Loads Qwen2.5-Omni-3B (~2 GB main model + ~1.5 GB mmproj) via
// `Chat.create({modelPath, mmprojPath, ...})` — the binding loads
// through the path-based ProjectionModel. Then asks the model to identify
// a penguin image passed through `Image.fromBytes(uint8)` (same MEMFS-
// backed mechanism).
//
// What's being validated end-to-end:
//   * Chat.create accepts mmprojPath alongside modelPath and produces
//     a worker with a working projection_model.
//   * Image.fromBytes wraps raw bytes in a tagged object whose worker-
//     side handler writes to /tmp/nbw-image-<hash>.bin in MEMFS and
//     pushes the resulting path through Prompt::push_image.
//   * `for await (const tok of chat.ask([...]))` runs the full
//     multimodal inference loop AND streams tokens — confirms the
//     per-token postMessage hook works with multimodal prompts, not
//     just text.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/tests/test_vision.mjs
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
console.log('  module loaded.');

// Sanity: Image factory is exposed.
assert.equal(typeof m.Image, 'function', 'expected m.Image factory');
assert.equal(typeof m.Image.fromBytes, 'function', 'expected m.Image.fromBytes');

console.log('\nReading image bytes...');
const imgBytes = new Uint8Array(readFileSync(imagePath));
console.log(`  image:      ${imgBytes.byteLength} bytes`);

// On wasm32 core::memory::plan_context floors n_ubatch at 1024 (not
// 2048 as on native) — keeps the compute buffer to ~600 MB and lets
// us afford a larger context. 4096 fits Qwen2-VL-2B's ~2500-token
// penguin image embedding + system prompt + reply, with margin.
console.log('\nBuilding Chat with mmproj (contextSize=4096 — fits the image embedding)...');
const t0 = performance.now();
const chat = await m.Chat.create({
  modelPath,
  mmprojPath,
  systemPrompt: 'You are a helpful assistant. Be brief.',
  templateVariables: { enable_thinking: false },
  contextSize: 4096,
});
console.log(`  Chat ready in ${(performance.now() - t0).toFixed(0)} ms`);

console.log('\nCalling chat.ask([text, Image.fromBytes(uint8)]) with for-await streaming...');
const tAsk = performance.now();
const img = m.Image.fromBytes(imgBytes);
console.log(`  Image part shape: __nbwKind=${img.__nbwKind}, ${img.bytes.byteLength} bytes`);

let chunkCount = 0;
let ttftMs = null;
let response = '';
for await (const tok of chat.ask([
  'What animal is in this image? One word, lowercase.',
  img,
])) {
  if (ttftMs === null) ttftMs = performance.now() - tAsk;
  chunkCount++;
  response += tok;
}
const dt = (performance.now() - tAsk) / 1000;

console.log(`\n=== Response (${dt.toFixed(1)} s) ===`);
console.log(response);
console.log(`\nStreaming stats: ${chunkCount} chunks, ttft=${(ttftMs ?? 0).toFixed(0)} ms`);

// Accept "tux" too: the small models used in CI (Qwen3.5-0.8B) sometimes
// name the Linux mascot rather than the species. Larger models say "penguin".
const lower = response.toLowerCase();
const identified = lower.includes('penguin') || lower.includes('tux');
console.log(`\nidentified penguin/tux: ${identified}`);
if (!identified) {
  console.error('FAIL: model did not identify a penguin (or tux)');
  process.exit(1);
}
// "penguin" is a single BPE token in most chat tokenizers but we
// asked for "one word, lowercase" — the model often adds a period
// or a sentence, so 2+ chunks are typical. Require at least 2 so
// we know the streaming path actually ran (vs. one buffered blob).
if (chunkCount < 2) {
  console.error(`FAIL: only ${chunkCount} chunk(s) — streaming did not engage on multimodal prompt`);
  process.exit(1);
}

chat.free();

console.log('\n=== Path A end-to-end vision passed ===');
console.log('  Chat.create({modelPath, mmprojPath}) wires mmproj through the path-based loader,');
console.log('  Image.fromBytes(uint8) wires image bytes through MEMFS,');
console.log('  multimodal inference returns a sensible answer,');
console.log('  AND streams it token-by-token via the per-token hook.');
