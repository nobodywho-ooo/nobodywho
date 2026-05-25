// wasm64 modelpath smoke. Mirrors modelpath-smoke.mjs but imports
// from pkg-bundler-wasm64/ — the memory64 artifacts produced by
// build-pkg-emscripten-wasm64.sh.
//
// Usage: node modelpath-smoke-wasm64.mjs <model.gguf> [mmproj.gguf]
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler-wasm64');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const modelPath = process.argv[2];
const mmprojPath = process.argv[3];
if (!modelPath) {
  console.error('usage: modelpath-smoke-wasm64.mjs <model.gguf> [mmproj.gguf]');
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

console.log(`[wasm64] modelPath=${modelPath}`);
if (mmprojPath) console.log(`         mmprojPath=${mmprojPath}`);

const t0 = performance.now();
const opts = {
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
};
if (mmprojPath) opts.mmprojPath = mmprojPath;

const chat = await m.Chat.create(opts);
console.log(`[ok] Chat.create resolved in ${(performance.now() - t0).toFixed(0)} ms`);

const askStart = performance.now();
let chunks = 0;
let text = '';
let ttftMs = null;
for await (const tok of chat.ask('Say hello in one short sentence.')) {
  if (ttftMs === null) ttftMs = performance.now() - askStart;
  chunks++;
  text += tok;
}
console.log(`[ok] ${chunks} chunks; ttft=${ttftMs.toFixed(0)} ms; total=${(performance.now() - askStart).toFixed(0)} ms`);
console.log(`     text: ${JSON.stringify(text)}`);

await chat.terminate();
console.log(`[ok] terminate() resolved`);

if (chunks < 2) {
  console.error(`FAIL: only ${chunks} chunk(s) — streaming did not engage`);
  process.exit(1);
}
console.log(`\nPASS: wasm64 build artifacts produce a working module`);
