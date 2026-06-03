// Test: Chat.create({modelPath}) — Node-only path-based loader that
// streams the model from host fs into MEMFS without holding a Buffer
// on the main thread.
//
// Asserts:
//   - Chat.create({modelPath}) resolves (worker successfully streamed
//     the file into MEMFS and loaded via the path-based loader)
//   - for-await on chat.ask(...) yields multiple tokens (streaming
//     code path unchanged)
//   - chat.terminate() resolves
//
// Optional second arg: mmprojPath (for multimodal models).
//
// Usage: node test_modelpath.mjs <model.gguf> [mmproj.gguf]
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const modelPath = process.argv[2];
const mmprojPath = process.argv[3];

if (!modelPath) {
  console.error('usage: test_modelpath.mjs <model.gguf> [mmproj.gguf]');
  process.exit(2);
}
if (!existsSync(modelPath)) {
  console.error(`model not found: ${modelPath}`);
  process.exit(2);
}
if (mmprojPath && !existsSync(mmprojPath)) {
  console.error(`mmproj not found: ${mmprojPath}`);
  process.exit(2);
}

const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

console.log(`[setup] modelPath=${modelPath}`);
if (mmprojPath) console.log(`        mmprojPath=${mmprojPath}`);

const t0 = performance.now();
const chatOpts = {
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
};
if (mmprojPath) chatOpts.mmprojPath = mmprojPath;

const chat = await m.Chat.create(chatOpts);
const createMs = performance.now() - t0;
console.log(`[ok] Chat.create resolved in ${createMs.toFixed(0)} ms (stream + load)`);

const askStart = performance.now();
let chunks = 0;
let text = '';
let ttftMs = null;
for await (const tok of chat.ask('Say hello in one short sentence.')) {
  if (ttftMs === null) ttftMs = performance.now() - askStart;
  chunks++;
  text += tok;
}
console.log(`[ok] for-await streamed ${chunks} chunks; ttft=${ttftMs.toFixed(0)} ms`);
console.log(`     text: ${JSON.stringify(text)}`);

await chat.terminate();
console.log(`[ok] terminate() resolved`);

if (chunks < 2) {
  console.error(`FAIL: only ${chunks} chunk(s) — streaming did not engage`);
  process.exit(1);
}
console.log(`\nPASS: modelPath end-to-end (no main-thread Buffer of model bytes)`);
