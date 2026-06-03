// Test for Chat.stopGeneration().
//
// Validates:
//   * Ask for a long generation, let a handful of tokens stream,
//     call stopGeneration(), confirm the stream resolves with fewer
//     tokens than the unstopped run would have produced.
//   * The chat remains usable: ask again on the same Chat and verify
//     it works (no zombie worker state from the stop).
//   * stopGeneration() is a no-op when no ask is in flight (no throw).
//   * stopGeneration() after terminate() is a silent no-op (matches
//     Python's behavior).
//
// Uses Qwen3-0.6B — small enough to iterate quickly.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/tests/test_stop.mjs

import { existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(here, '..', 'pkg-bundler');

const modelPath = process.argv[2]
  ?? '/Users/user/Library/Caches/nobodywho/models/NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf';
if (!existsSync(modelPath)) {
  console.error(`missing model: ${modelPath}`);
  process.exit(2);
}

console.log('Loading wasm...');
let __hookDebugSeen = 0;
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

console.log('\n[1] Long generation, stop after a few tokens...');
const PROMPT = 'Write a long, detailed essay about the history of Copenhagen.';
const STOP_AFTER = 5;

// Baseline: how many tokens this prompt+model produces WITHOUT stopping
// (capped so a runaway can't hang the test). We compare the stopped run
// against THIS, not an arbitrary fixed token ceiling — otherwise a
// naturally-short generation would pass even if stopGeneration() were a
// no-op (the original `count < 200` check did exactly that).
const BASELINE_CAP = 200;
const baselineChat = await m.Chat.create({
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
});
let baseline = 0;
for await (const _tok of baselineChat.ask(PROMPT)) {
  if (++baseline >= BASELINE_CAP) break;
}
await baselineChat.terminate();
console.log(`    unstopped baseline produced ${baseline} tokens (cap ${BASELINE_CAP}).`);
assert.ok(baseline > STOP_AFTER * 4, `baseline too short (${baseline}) to meaningfully test stop — prompt under-generated`);

const chat = await m.Chat.create({
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
});
const stream = chat.ask(PROMPT);
let count = 0;
const start = performance.now();
let stoppedAt = null;
for await (const tok of stream) {
  count++;
  if (count === STOP_AFTER) {
    stoppedAt = performance.now() - start;
    chat.stopGeneration();
    console.log(`    stopGeneration() called after ${count} tokens (${stoppedAt.toFixed(0)} ms)`);
  }
}
const totalMs = performance.now() - start;
console.log(`    Stream ended at ${count} tokens (${totalMs.toFixed(0)} ms total).`);
const post_stop_tokens = count - STOP_AFTER;
console.log(`    Tokens after stopGeneration(): ${post_stop_tokens} (in-flight tail; expected small).`);
// A working stop produces far fewer tokens than the unstopped baseline and
// only a small in-flight tail; a no-op stop would run to ~baseline and fail
// both assertions.
assert.ok(count < baseline, `expected stopped run (${count}) < unstopped baseline (${baseline}); stopGeneration() looks like a no-op`);
assert.ok(post_stop_tokens < baseline / 2, `expected a small in-flight tail after stop, got ${post_stop_tokens} (baseline ${baseline})`);
assert.ok(count >= STOP_AFTER, `expected at least ${STOP_AFTER} tokens (got ${count}); race between stop and stream consumption`);
console.log('    ✓ stopGeneration cut the generation short');

console.log('\n[2] Chat is reusable after stop — second ask works...');
const second = await chat.ask('Say "hello" once.').completed();
console.log(`    second ask response: ${JSON.stringify(second).slice(0, 80)}`);
assert.ok(second.length > 0, 'expected non-empty response from second ask');
console.log('    ✓ chat survived the stop, second ask completed');

console.log('\n[3] stopGeneration() when no ask is in flight — no throw...');
chat.stopGeneration();
console.log('    ✓ no error');

console.log('\n[4] Terminate, then stopGeneration() — silent no-op...');
await chat.terminate();
chat.stopGeneration();
console.log('    ✓ no error after terminate');

console.log('\n=== stop passed ===');
process.exit(0);
