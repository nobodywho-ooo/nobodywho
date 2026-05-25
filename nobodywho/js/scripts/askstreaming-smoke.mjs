// Smoke test for Chat.askStreaming(prompt, callback) — the callback
// shape mirroring the older JS-bridge binding's API.
//
// Validates:
//   * callback fires per token (count > 0 and matches stream of asks)
//   * concatenated tokens equal the resolved full text
//   * Promise resolves to the full response string
//   * second ask works after the first completes (chat is reusable)
//   * concurrent askStreaming call rejects with the "another ask in
//     progress" error (matches Chat.ask's contract)
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/askstreaming-smoke.mjs

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
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();
const modelBytes = new Uint8Array(readFileSync(modelPath));

const chat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply concisely.',
  templateVariables: { enable_thinking: false },
});

console.log('\n[1] askStreaming fires callback per token, returns full text...');
const tokens = [];
const full = await chat.askStreaming('Say "ack" then stop.', (tok) => {
  tokens.push(tok);
});
console.log(`    tokens received: ${tokens.length}`);
console.log(`    full text: ${JSON.stringify(full).slice(0, 80)}`);
assert.ok(tokens.length > 0, 'expected at least one token');
assert.equal(tokens.join(''), full, 'concatenated tokens must equal the resolved full text');
assert.ok(full.length > 0, 'expected non-empty resolved text');
console.log('    ✓ per-token callback fired; full text consistent');

console.log('\n[2] chat reusable — a second askStreaming after the first...');
const tokens2 = [];
const full2 = await chat.askStreaming('Say "ok".', (tok) => tokens2.push(tok));
console.log(`    tokens2: ${tokens2.length}, full2: ${JSON.stringify(full2).slice(0, 40)}`);
assert.ok(tokens2.length > 0);
assert.equal(tokens2.join(''), full2);
console.log('    ✓ second ask completed');

console.log('\n[3] concurrent askStreaming rejects with "another ask in progress"...');
const first = chat.askStreaming('Say "one".', () => {});
let secondErr = null;
try {
  await chat.askStreaming('Say "two".', () => {});
} catch (e) {
  secondErr = e.message ?? String(e);
}
console.log(`    second-call error: ${secondErr}`);
assert.ok(secondErr && /another ask is in progress/i.test(secondErr),
  `expected "another ask is in progress" error, got: ${secondErr}`);
await first;  // let the in-flight one finish before terminate
console.log('    ✓ concurrent call rejected cleanly');

await chat.terminate();

console.log('\n=== askstreaming-smoke passed ===');
process.exit(0);
