// Sampler-config smoke test for the JS binding.
//
// Validates:
//   * `sampler: SamplerPresets.greedy()` produces deterministic output:
//     two runs with the same prompt yield the exact same token sequence.
//   * `sampler: new SamplerBuilder().temperature(...).topK(...).dist()`
//     accepts the fields without erroring at construction time.
//   * Combining sampler with constraint via SamplerPresets doesn't break
//     anything.
//
// Runs through `Chat.create({modelPath, ...})` so the same smoke
// exercises both browser and Node (Node uses `worker_threads` under the
// hood via `__nbw_spawn_worker`). Each section spawns a fresh Chat
// and terminates it when done so workers don't pile up.
//
// Uses Qwen3-0.6B — small enough to iterate quickly.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/sampler-smoke.mjs

import { existsSync } from 'node:fs';
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

const PROMPT = 'Reply with exactly one word: hello';

async function runGreedyOnce() {
  const chat = await m.Chat.create({
    modelPath,
    systemPrompt: 'Reply briefly.',
    templateVariables: { enable_thinking: false },
    sampler: m.SamplerPresets.greedy(),
  });
  try {
    return await chat.ask(PROMPT).completed();
  } finally {
    await chat.terminate();
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
  modelPath,
  systemPrompt: 'You are helpful.',
  templateVariables: { enable_thinking: false },
  sampler: new m.SamplerBuilder()
    .temperature(0.7)
    .topK(40)
    .topP(0.95, 1)
    .minP(0.05, 1)
    .penalties(1.1, 64, 0.0, 0.0)
    .dist(),
});
await customChat.terminate();
console.log('    ✓ constructed without error');

// === 3. Sampler + constraint compose ===
console.log('\n[3] sampler + constraint together (constraint via SamplerPresets)...');
const composedChat = await m.Chat.create({
  modelPath,
  systemPrompt: 'Reply with exactly one word.',
  templateVariables: { enable_thinking: false },
  sampler: m.SamplerPresets.constrainWithRegex('[a-z]+'),
});
await composedChat.terminate();
console.log('    ✓ constructed without error');

console.log('\n=== Sampler-config JS smoke test passed ===');
console.log('  Greedy sampling is deterministic across two Chats.');
console.log('  Custom SamplerBuilder chain with temperature/topK/topP/minP/penalties accepted.');
console.log('  Sampler + constraint compose without conflict.');

process.exit(0);
