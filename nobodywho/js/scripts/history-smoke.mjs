// Smoke test for Chat.getChatHistory() / Chat.setChatHistory().
//
// Validates:
//   * After an ask, getChatHistory returns user + assistant messages.
//   * setChatHistory replaces the history; subsequent getChatHistory
//     reflects the new state.
//   * Loaded history is actually used as context — ask a question that
//     only makes sense given the loaded history, verify the model
//     answers as if it had the prior turn in context.
//
// Uses Qwen3-0.6B — small enough to iterate quickly.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/history-smoke.mjs

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

const modelBytes = new Uint8Array(readFileSync(modelPath));

console.log('\n[1] Fresh chat — history empty before any ask...');
const chat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply briefly.',
  templateVariables: { enable_thinking: false },
});
const empty = await chat.getChatHistory();
console.log(`    initial history length: ${empty.length}`);
assert.equal(empty.length, 0, 'fresh chat history should be empty (system prompt is excluded)');
console.log('    ✓ empty');

console.log('\n[2] After ask — history has user + assistant...');
const reply = await chat.ask('Say "ack".').completed();
console.log(`    reply: ${JSON.stringify(reply).slice(0, 60)}`);
const afterAsk = await chat.getChatHistory();
console.log(`    history length: ${afterAsk.length}`);
console.log(`    history: ${JSON.stringify(afterAsk).slice(0, 200)}`);
assert.equal(afterAsk.length, 2, 'expected user + assistant entries');
const userMsg = afterAsk.find((m) => m.role === 'user') ?? afterAsk[0];
const asstMsg = afterAsk.find((m) => m.role === 'assistant') ?? afterAsk[1];
assert.ok(userMsg, 'user message present');
assert.ok(asstMsg, 'assistant message present');
console.log('    ✓ user + assistant present');

console.log('\n[3] setChatHistory replaces — round-trip set/get...');
const loaded = [
  { role: 'user', content: 'My favorite color is purple.' },
  { role: 'assistant', content: 'Noted — purple it is.' },
];
await chat.setChatHistory(loaded);
const got = await chat.getChatHistory();
console.log(`    after set: ${JSON.stringify(got)}`);
assert.equal(got.length, 2, 'expected two messages after set');
assert.equal(got[0].content, 'My favorite color is purple.');
assert.equal(got[1].content, 'Noted — purple it is.');
console.log('    ✓ round-trip matches');

console.log('\n[4] Loaded history is actually used as context — ask about the color...');
const colorReply = await chat.ask('What is my favorite color? Reply with one word.').completed();
console.log(`    reply: ${JSON.stringify(colorReply).slice(0, 80)}`);
assert.ok(
  colorReply.toLowerCase().includes('purple'),
  `model didn't use loaded history — expected "purple" in response, got: ${colorReply}`,
);
console.log('    ✓ model used loaded history');

await chat.terminate();

console.log('\n=== history-smoke passed ===');
process.exit(0);
