// Smoke test for the extended sampler surface — closes the last Python
// parity gaps on samplers:
//   * SamplerSpec accepts DRY / XTC / TypicalP shift fields.
//   * SamplerBuilder has .dry / .xtc / .typicalP / .penalties methods.
//   * SamplerPresets has .dry() and .json() presets.
//
// Sanity-only: we don't try to assert each sampler produces specific
// output — sampler-config behavior is verified by core's tests. Here
// we just confirm the JS layer accepts the spec and Chat.create
// instantiates a usable chat for each shape.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/sampler-extra-smoke.mjs

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

async function runWithSampler(label, sampler) {
  const chat = await m.Chat.create({
    modelBytes,
    systemPrompt: 'Reply with the single word "ok".',
    templateVariables: { enable_thinking: false },
    sampler,
  });
  const reply = await chat.ask('Say "ok".').completed();
  console.log(`    ${label}: ${JSON.stringify(reply).slice(0, 50)}`);
  assert.ok(reply.length > 0, `${label}: expected non-empty reply`);
  await chat.terminate();
}

console.log('\n[1] SamplerBuilder.dry(...) — DRY repetition penalty...');
const drySampler = new m.SamplerBuilder()
  .topK(40)
  .dry(0.8, 1.75, 2, -1, ['\n', ':'])
  .dist();
console.log(`    spec: ${JSON.stringify(drySampler).slice(0, 200)}`);
assert.equal(drySampler.dryMultiplier.toFixed(2), '0.80');
assert.deepEqual(drySampler.drySeqBreakers, ['\n', ':']);
await runWithSampler('dry-sampler', drySampler);
console.log('    ✓ DRY accepted');

console.log('\n[2] SamplerBuilder.xtc(...) — XTC sampling...');
const xtcSampler = new m.SamplerBuilder().xtc(0.3, 0.1, 1).dist();
console.log(`    spec: ${JSON.stringify(xtcSampler)}`);
assert.equal(xtcSampler.xtcProbability.toFixed(2), '0.30');
await runWithSampler('xtc-sampler', xtcSampler);
console.log('    ✓ XTC accepted');

console.log('\n[3] SamplerBuilder.typicalP(...) — Typical-P sampling...');
const typSampler = new m.SamplerBuilder().typicalP(0.9, 1).dist();
console.log(`    spec: ${JSON.stringify(typSampler)}`);
assert.equal(typSampler.typicalP.toFixed(2), '0.90');
await runWithSampler('typicalP-sampler', typSampler);
console.log('    ✓ TypicalP accepted');

console.log('\n[4] SamplerBuilder.penalties(...) — full repeat-penalty step...');
const penSampler = new m.SamplerBuilder()
  .penalties(1.1, 64, 0.05, 0.05)
  .dist();
console.log(`    spec: ${JSON.stringify(penSampler)}`);
assert.equal(penSampler.repeatPenalty.toFixed(2), '1.10');
assert.equal(penSampler.repeatFreqPenalty.toFixed(2), '0.05');
assert.equal(penSampler.repeatPresentPenalty.toFixed(2), '0.05');
await runWithSampler('penalties-sampler', penSampler);
console.log('    ✓ Penalties accepted');

console.log('\n[5] SamplerPresets.dry() — preset shape...');
const dryPreset = m.SamplerPresets.dry();
console.log(`    spec: ${JSON.stringify(dryPreset)}`);
assert.equal(dryPreset.dryMultiplier, 0);
assert.equal(dryPreset.dryBase.toFixed(2), '1.75');
assert.equal(dryPreset.dryAllowedLength, 2);
assert.deepEqual(dryPreset.drySeqBreakers, ['\n', ':', '"', '*']);
console.log('    ✓ dry() preset shape OK');

console.log('\n[6] SamplerPresets.json() — JSON-constrained preset shape + actually JSON...');
const jsonPreset = m.SamplerPresets.json();
console.log(`    spec: ${JSON.stringify(jsonPreset)}`);
assert.deepEqual(jsonPreset, { constraint: { jsonSchema: '{}' } });
const jchat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply with valid JSON only.',
  templateVariables: { enable_thinking: false },
  ...jsonPreset,
});
const jreply = await jchat.ask('Give a JSON object with one key "color" set to a color name.').completed();
console.log(`    constrained reply: ${JSON.stringify(jreply).slice(0, 80)}`);
// Must parse as JSON to count.
try {
  JSON.parse(jreply.trim());
  console.log('    ✓ output parses as JSON');
} catch (e) {
  assert.fail(`expected JSON-parseable output, got: ${jreply}`);
}
await jchat.terminate();

console.log('\n=== sampler-extra-smoke passed ===');
process.exit(0);
