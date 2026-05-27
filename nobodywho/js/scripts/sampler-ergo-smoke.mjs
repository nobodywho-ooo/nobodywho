// Smoke test for SamplerBuilder + SamplerPresets ergonomic wrappers.
//
// Validates:
//   * SamplerPresets.greedy() / .temperature() / .topK() / .topP() /
//     .default() return the expected JS shapes.
//   * SamplerPresets.constrainWithRegex() returns a sampler-shaped
//     {constraint:{regex}} object and a constrained ask produces only
//     matching tokens.
//   * SamplerBuilder fluent chain produces a sampler spec equivalent to
//     hand-writing the JSON; Chat.create accepts it and ask works.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/sampler-ergo-smoke.mjs

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

console.log('\n[1] SamplerPresets shape sanity...');
const greedy = m.SamplerPresets.greedy();
console.log(`    greedy(): ${JSON.stringify(greedy)}`);
assert.deepEqual(greedy, { sampleStep: 'greedy' });

const t = m.SamplerPresets.temperature(0.8);
console.log(`    temperature(0.8): ${JSON.stringify(t)}`);
assert.equal(t.temperature.toFixed(3), '0.800');

const k = m.SamplerPresets.topK(40);
console.log(`    topK(40): ${JSON.stringify(k)}`);
assert.deepEqual(k, { topK: 40 });

const p = m.SamplerPresets.topP(0.95);
console.log(`    topP(0.95): ${JSON.stringify(p)}`);
assert.equal(p.topP.toFixed(3), '0.950');

const def = m.SamplerPresets.default();
console.log(`    default(): ${JSON.stringify(def)}`);
assert.deepEqual(def, {});

const reg = m.SamplerPresets.constrainWithRegex('^\\d+$');
console.log(`    constrainWithRegex: ${JSON.stringify(reg)}`);
assert.deepEqual(reg, { constraint: { regex: '^\\d+$' } });
console.log('    ✓ presets shapes match');

console.log('\n[2] SamplerBuilder fluent chain shape...');
const sb = new m.SamplerBuilder()
  .topK(40)
  .topP(0.95)
  .temperature(0.7)
  .dist();
console.log(`    built: ${JSON.stringify(sb)}`);
assert.equal(sb.topK, 40);
assert.equal(sb.topP.toFixed(3), '0.950');
assert.equal(sb.temperature.toFixed(3), '0.700');
assert.equal(sb.sampleStep, 'dist');
console.log('    ✓ fluent chain shape OK');

console.log('\n[3] SamplerBuilder.greedy() — terminal...');
const sbg = new m.SamplerBuilder().greedy();
console.log(`    greedy chain: ${JSON.stringify(sbg)}`);
assert.equal(sbg.sampleStep, 'greedy');
console.log('    ✓ greedy terminal OK');

console.log('\n[4] Chat.create accepts the built sampler — actually runs inference...');
const chat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply concisely.',
  templateVariables: { enable_thinking: false },
  sampler: new m.SamplerBuilder().topK(40).temperature(0.5).dist(),
});
const reply = await chat.ask('Say "ok".').completed();
console.log(`    reply: ${JSON.stringify(reply).slice(0, 60)}`);
assert.ok(reply.length > 0);
await chat.terminate();
console.log('    ✓ Chat.create + built sampler + ask round-trip');

console.log('\n[5] SamplerPresets.constrainWithRegex routed through Chat.create...');
const chat2 = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply with a single integer.',
  templateVariables: { enable_thinking: false },
  sampler: m.SamplerPresets.constrainWithRegex('^[0-9]{1,3}$'),
});
const numReply = await chat2.ask('Pick a number 1-100.').completed();
console.log(`    constrained reply: ${JSON.stringify(numReply).slice(0, 30)}`);
assert.ok(/^[0-9]{1,3}$/.test(numReply.trim()),
  `constrained output must match regex, got: ${numReply}`);
await chat2.terminate();
console.log('    ✓ regex constraint enforced');

console.log('\n=== sampler-ergo-smoke passed ===');
process.exit(0);
