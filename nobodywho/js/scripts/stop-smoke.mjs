// Smoke test for Chat.stopGeneration().
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
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/stop-smoke.mjs

import { readFileSync, existsSync } from 'node:fs';
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
m.init();

const modelBytes = new Uint8Array(readFileSync(modelPath));

console.log('\n[1] Long generation, stop after a few tokens...');
const chat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply concisely.',
  templateVariables: { enable_thinking: false },
});

// Ask for something long. Sample tokens as they arrive; stop after
// we've seen STOP_AFTER tokens; confirm the stream resolves with a
// final count not vastly greater (some in-flight tokens land between
// the stop call and core's loop noticing the flag).
const STOP_AFTER = 5;
const stream = chat.ask('Write a 500-word essay about Copenhagen.');
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
assert.ok(count < 200, `expected stopped run to produce <200 tokens, got ${count}`);
assert.ok(count >= STOP_AFTER, `expected at least ${STOP_AFTER} tokens (got ${count}); race between stop and stream consumption`);
const post_stop_tokens = count - STOP_AFTER;
console.log(`    Tokens that landed after stopGeneration(): ${post_stop_tokens} (in-flight tail; expected small)`);
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

console.log('\n=== stop-smoke passed ===');
process.exit(0);
