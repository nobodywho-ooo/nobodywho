// Sampler-config smoke test for the JS binding.
//
// Validates:
//   * `sampler: { sampleStep: 'greedy' }` produces deterministic output:
//     two runs with the same prompt yield the exact same token sequence.
//   * `sampler: { temperature: ..., topK: ..., topP: ... }` accepts the
//     fields without erroring at construction time.
//   * `sampler: { sampleStep: 'bogus' }` rejects with a clear error.
//   * Combining `sampler` and `constraint` doesn't break anything (constraint
//     prepends a shift step to the user's sampler chain).
//
// Runs through `WorkerChat.create({modelBytes, ...})` so the same smoke
// exercises both browser and Node (Node uses `worker_threads` under the
// hood via `__nbw_spawn_worker`). Each section spawns a fresh WorkerChat
// and terminates it when done so workers don't pile up.
//
// Uses Qwen3-0.6B — small enough to iterate quickly.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/sampler-smoke.mjs

import { readFileSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(here, '..', 'pkg-bundler');

const modelPath = process.argv[2]
  ?? '/nix/store/6i6yqpaz8ikxyi3lkmxj9zgwjdjsmwgi-Qwen_Qwen3-0.6B-Q4_K_M.gguf';
if (!existsSync(modelPath)) { console.error('missing model:', modelPath); process.exit(2); }

console.log('Loading wasm...');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();

const modelBytes = new Uint8Array(readFileSync(modelPath));

const PROMPT = 'Reply with exactly one word: hello';

async function runGreedyOnce() {
  const chat = await m.Chat.create({
    modelBytes,
    systemPrompt: 'Reply briefly.',
    templateVariables: { enable_thinking: false },
    sampler: { sampleStep: 'greedy' },
  });
  try {
    return await chat.ask(PROMPT).completed();
  } finally {
    chat.terminate();
  }
}

// === 1. Greedy sampling is deterministic ===
console.log('\n[1] Greedy sampling: two runs with same prompt → identical output...');
const greedy1 = await runGreedyOnce();
const greedy2 = await runGreedyOnce();
console.log(`    run 1: ${JSON.stringify(greedy1.slice(0, 80))}`);
console.log(`    run 2: ${JSON.stringify(greedy2.slice(0, 80))}`);
assert.equal(
  greedy1,
  greedy2,
  `greedy sampling should be deterministic; got differing outputs`,
);
console.log('    ✓ identical');

// === 2. Temperature / topK / topP accepted at construction time ===
console.log('\n[2] Custom sampler with temperature/topK/topP/minP/repeatPenalty...');
const customChat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'You are helpful.',
  templateVariables: { enable_thinking: false },
  sampler: {
    temperature: 0.7,
    topK: 40,
    topP: 0.95,
    minP: 0.05,
    repeatPenalty: 1.1,
    repeatLastN: 64,
    sampleStep: 'dist',
  },
});
customChat.terminate();
console.log('    ✓ constructed without error');

// === 3. Invalid sampleStep rejects ===
console.log('\n[3] Invalid sampleStep rejects with clear error...');
let threw = false;
try {
  await m.Chat.create({ modelBytes, sampler: { sampleStep: 'bogus' } });
} catch (e) {
  threw = true;
  console.log(`    caught: ${e.message ?? e}`);
  assert.match(String(e.message ?? e), /sampleStep/i);
  assert.match(String(e.message ?? e), /bogus/i);
}
assert.ok(threw, 'expected invalid sampleStep to reject the create Promise');
console.log('    ✓ rejected');

// === 4. Sampler + constraint compose ===
console.log('\n[4] sampler + constraint together (constraint prepended to chain)...');
const composedChat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply with exactly one word.',
  templateVariables: { enable_thinking: false },
  sampler: { temperature: 0.5, topP: 0.9, sampleStep: 'dist' },
  constraint: { regex: '[a-z]+' },
});
composedChat.terminate();
console.log('    ✓ constructed without error');

console.log('\n=== Sampler-config JS smoke test passed ===');
console.log('  Greedy sampling is deterministic across two WorkerChats.');
console.log('  Custom temperature/topK/topP/minP/repeatPenalty fields accepted.');
console.log('  Invalid sampleStep rejects the create Promise with a clear error.');
console.log('  Sampler + constraint compose without conflict.');
