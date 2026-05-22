// Constraint (structured-output) smoke test for the JS binding.
//
// Validates that `constraint: { regex | jsonSchema | lark }` on
// `Chat.create({...})` actually constrains generation at the token
// sampler level. llguidance (the upstream sampler) compiles the
// constraint to a token-level grammar; under Emscripten this runs
// through the standard llama.cpp llguidance integration.
//
// Sections (each gets its own Chat — sampler config is fixed at construction):
//   1. Regex: response matches `[A-Z][a-z]+` (single capitalized word).
//   2. JSON Schema: response is a JSON object with `city` and `country`.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/constraint-smoke.mjs

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

// === 1. Regex constraint ===
console.log('\n[1] Regex constraint `^[A-Z][a-z]+$` (single capitalized word)...');
const regex = /^[A-Z][a-z]+$/;
const regexChat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply with one word.',
  templateVariables: { enable_thinking: false },
  constraint: { regex: '[A-Z][a-z]+' },
});
const t0 = performance.now();
const regexResponse = await regexChat.ask('Name a city.').completed();
const dt1 = ((performance.now() - t0) / 1000).toFixed(1);
console.log(`    response (${dt1} s): ${JSON.stringify(regexResponse)}`);
assert.match(
  regexResponse,
  regex,
  `regex constraint should produce a single capitalized word; got: ${JSON.stringify(regexResponse)}`,
);
console.log(`    ✓ matches /${regex.source}/`);
await regexChat.terminate();

// === 2. JSON Schema constraint ===
console.log('\n[2] JSON Schema constraint (object with `city` and `country` strings)...');
const schema = {
  type: 'object',
  properties: {
    city: { type: 'string' },
    country: { type: 'string' },
  },
  required: ['city', 'country'],
  additionalProperties: false,
};
const jsonChat = await m.Chat.create({
  modelBytes,
  systemPrompt: 'Reply with one short JSON object.',
  templateVariables: { enable_thinking: false },
  constraint: { jsonSchema: JSON.stringify(schema) },
});
const t1 = performance.now();
const jsonResponse = await jsonChat.ask('Give me a city and its country.').completed();
const dt2 = ((performance.now() - t1) / 1000).toFixed(1);
console.log(`    response (${dt2} s): ${jsonResponse}`);
let parsed;
try {
  parsed = JSON.parse(jsonResponse);
} catch (e) {
  assert.fail(`response is not valid JSON: ${jsonResponse}\nerror: ${e.message}`);
}
assert.equal(typeof parsed.city, 'string', `parsed.city must be a string; got ${typeof parsed.city}`);
assert.equal(typeof parsed.country, 'string', `parsed.country must be a string; got ${typeof parsed.country}`);
console.log(`    ✓ parsed: city=${JSON.stringify(parsed.city)} country=${JSON.stringify(parsed.country)}`);
await jsonChat.terminate();

console.log('\n=== Constraint JS smoke test passed ===');
console.log('  Regex and JSON Schema constraints both shape token sampling');
console.log('  end-to-end through llguidance on Emscripten.');

process.exit(0);
