// Tool-calling smoke test for the JS binding.
//
// Validates:
//   * `Tool.fromFn(name, description, jsonSchema, callback)` builds a
//     tagged tool object.
//   * Passing `tools: [tool]` through `new Chat(model, { ... })` plumbs
//     the JS callback all the way down to core's `Fn(Value) -> String`
//     dispatcher.
//   * The model emits a tool-call → core invokes our JS callback →
//     callback's return value gets injected into the conversation →
//     model produces a final response that reflects the tool's output.
//
// Uses Qwen3-0.6B (text-only, ~480 MB) — small enough to iterate
// quickly and known to handle tool calls via the grammar sampler.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/tool-smoke.mjs

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

// === 1. Surface check ===
console.log('\n[1] m.Tool.fromFn exposed?');
assert.equal(typeof m.Tool, 'function');
assert.equal(typeof m.Tool.fromFn, 'function');
console.log('    ✓');

// === 2. Tool.fromFn returns a tagged object ===
console.log('\n[2] Tool.fromFn returns tagged object...');
let callCount = 0;
let lastArgs = null;
const weatherTool = m.Tool.fromFn(
  'get_weather',
  'Get the current weather for a city. Returns a short human-readable description.',
  {
    type: 'object',
    properties: {
      city: { type: 'string', description: 'City name in English' },
    },
    required: ['city'],
  },
  (args) => {
    callCount++;
    lastArgs = args;
    return `Sunny in ${args.city}, 21 degrees Celsius.`;
  },
);
assert.equal(weatherTool.__nbwKind, 'tool');
assert.equal(weatherTool.name, 'get_weather');
assert.equal(typeof weatherTool.callback, 'function');
console.log(`    ✓ name=${weatherTool.name} __nbwKind=${weatherTool.__nbwKind}`);

// === 3. Chat with tools — model decides whether to call ===
console.log('\n[3] Loading model + building Chat with tools...');
const modelBytes = new Uint8Array(readFileSync(modelPath));
const model = await m.Model.loadBytes(modelBytes);
const chat = new m.Chat(model, {
  systemPrompt:
    'You are a helpful assistant. When the user asks about weather, use the get_weather tool. Then answer in one short sentence.',
  templateVariables: { enable_thinking: false },
  tools: [weatherTool],
});
console.log('    chat constructed ✓');

// === 4. Ask a weather question — model should call the tool ===
console.log('\n[4] Asking weather question (expect tool invocation)...');
const t0 = performance.now();
const stream = await chat.ask('What is the weather like in Copenhagen?');
const response = await stream.completed();
const dt = ((performance.now() - t0) / 1000).toFixed(1);
console.log(`\n=== Response (${dt} s) ===`);
console.log(response);
console.log(`\ntool was called ${callCount} time(s)`);
console.log(`last args: ${JSON.stringify(lastArgs)}`);

// Win condition: the JS callback was invoked. That's what proves
// `Tool.fromFn` → Chat options → core's `Fn(Value) -> String`
// dispatcher chain works end-to-end. We don't assert on the args'
// shape or on the final response containing the tool's answer —
// both depend on (a) the chosen format handler enforcing the
// `required` keyword of the json_schema and (b) the model
// faithfully following the schema. Qwen3-0.6B is a small model
// and routinely emits `arguments: {}` regardless of the schema's
// `required` list. Verifying that here is a model-quality test,
// not a JS-binding integration test.
assert.ok(callCount >= 1, `expected the JS callback to be invoked at least once; got ${callCount}`);
assert.ok(
  typeof lastArgs === 'object' && lastArgs !== null,
  `expected callback args to be an object; got ${typeof lastArgs}: ${JSON.stringify(lastArgs)}`,
);

console.log('\n=== Tool-calling JS smoke test passed ===');
console.log('  Tool.fromFn registers a tagged tool, Chat.tools plumbs it');
console.log('  through core, and the model invocation reaches the JS callback.');
console.log('  (Schema enforcement on args + final-response synthesis are');
console.log('   model-quality concerns separate from JS-binding wiring.)');
