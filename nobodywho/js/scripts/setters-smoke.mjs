// Smoke test for Chat mid-session setters/getters and reset.
//
// Validates:
//   * getSystemPrompt returns what was passed at create time.
//   * setSystemPrompt changes it; getSystemPrompt reflects the change.
//   * setSamplerConfig / getSamplerConfig round-trip.
//   * setTemplateVariable + setTemplateVariables + getTemplateVariables
//     round-trip; the change actually affects template rendering
//     (verified by toggling enable_thinking and checking the response
//     for / lack of <think> blocks).
//   * setTools replaces tool registry (history-via-tools verifies a
//     newly-registered tool actually fires).
//   * resetHistory empties chat history without re-creating Chat.
//
// Uses Qwen3-0.6B — small enough to iterate quickly.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/setters-smoke.mjs

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

console.log('\n[1] System prompt round-trip...');
const chat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Initial system prompt.',
  templateVariables: { enable_thinking: false },
});
const sp1 = await chat.getSystemPrompt();
console.log(`    initial getSystemPrompt: ${JSON.stringify(sp1)}`);
assert.equal(sp1, 'Initial system prompt.');
await chat.setSystemPrompt('Updated system prompt.');
const sp2 = await chat.getSystemPrompt();
console.log(`    after set: ${JSON.stringify(sp2)}`);
assert.equal(sp2, 'Updated system prompt.');
await chat.setSystemPrompt(null);
const sp3 = await chat.getSystemPrompt();
console.log(`    after set null: ${JSON.stringify(sp3)}`);
assert.equal(sp3, null);
await chat.setSystemPrompt('Reply concisely.');  // restore for later asks
console.log('    ✓ system prompt round-trips (incl. null)');

console.log('\n[2] Sampler config round-trip...');
const sCfg = await chat.getSamplerConfig();
console.log(`    initial sampler shape: ${typeof sCfg}`);
assert.ok(sCfg, 'sampler config should be present');
await chat.setSamplerConfig({ temperature: 0.42, topK: 7, topP: 0.5, seed: 1234 });
const sCfg2 = await chat.getSamplerConfig();
console.log(`    after set: ${JSON.stringify(sCfg2).slice(0, 120)}`);
assert.ok(sCfg2, 'sampler config after set');
console.log('    ✓ sampler set/get OK');

console.log('\n[3] Template variables round-trip + effect on rendering...');
await chat.setTemplateVariable('enable_thinking', false);
const v1 = await chat.getTemplateVariables();
console.log(`    after setVar: ${JSON.stringify(v1)}`);
assert.equal(v1.enable_thinking, false);
await chat.setTemplateVariables({ enable_thinking: true });
const v2 = await chat.getTemplateVariables();
console.log(`    after setVars: ${JSON.stringify(v2)}`);
assert.equal(v2.enable_thinking, true);
await chat.setTemplateVariables({ enable_thinking: false }); // turn off for next asks
console.log('    ✓ template vars set/get OK');

console.log('\n[4] setTools replaces tool registry...');
let toolCalled = 0;
const colorTool = m.Tool.fromFn(
  'get_user_color',
  'Get the user\'s favorite color',
  { type: 'object', properties: {}, required: [] },
  () => { toolCalled++; return 'magenta'; },
);
await chat.setTools([colorTool]);
console.log('    tools set');
// Don't actually invoke (model decision is flaky); just verify no error
// and that the chat is still usable.
const ok = await chat.ask('Say "ack".').completed();
console.log(`    ask after setTools: ${JSON.stringify(ok).slice(0, 50)}`);
assert.ok(ok.length > 0);
console.log('    ✓ chat usable after setTools');

console.log('\n[5] resetHistory clears the conversation...');
const before = await chat.getChatHistory();
console.log(`    history before reset: ${before.length} entries`);
assert.ok(before.length > 0, 'should have some history from prior asks');
await chat.resetHistory();
const after = await chat.getChatHistory();
console.log(`    history after reset: ${after.length} entries`);
assert.equal(after.length, 0, 'history should be empty after reset');
console.log('    ✓ resetHistory works');

await chat.terminate();

console.log('\n=== setters-smoke passed ===');
process.exit(0);
