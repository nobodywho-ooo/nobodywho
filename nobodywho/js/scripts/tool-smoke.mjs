// Tool-calling smoke test for the JS binding.
//
// Validates:
//   * `Tool.fromFn(name, description, jsonSchema, callback)` builds a
//     tagged tool object.
//   * Passing `tools: [tool]` through `Chat.create({...})` plumbs
//     the JS callback all the way down through the worker-side tool RPC
//     bridge: worker emits `tool-call` postMessage → main looks up the
//     callback → invokes it (awaiting if it returns a Promise) → posts
//     `tool-reply` back → worker resumes inference with the result.
//   * Both sync and Promise-returning callbacks resolve correctly. The
//     async path is the proof that the wasm correctly yields to the JS
//     event loop while parked at the tool-dispatch boundary.
//
// Uses Qwen3-0.6B (text-only, ~480 MB) — small enough to iterate
// quickly and known to handle tool calls via the grammar sampler.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/tool-smoke.mjs

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
console.log('\n[3] Spawning Chat with tools...');
const chat = await m.Chat.create({
  modelPath,
  systemPrompt:
    'You are a helpful assistant. When the user asks about weather, use the get_weather tool. Then answer in one short sentence.',
  templateVariables: { enable_thinking: false },
  tools: [weatherTool],
});
console.log('    Chat ready ✓');

// === 4. Ask a weather question — model should call the tool ===
console.log('\n[4] Asking weather question (expect tool invocation)...');
const t0 = performance.now();
const stream = chat.ask('What is the weather like in Copenhagen?');
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
// Round-trip proof: a non-empty FINAL response means the worker received the
// tool-reply and RESUMED inference past the tool-dispatch boundary. We do NOT
// assert the answer *contains* the tool's text — with a 0.6B model that's a
// flaky model-quality check (per the note above); a non-empty completion is
// the binding-level guarantee that the reply actually made it back.
assert.ok(
  response.length > 0,
  `expected a non-empty final response after the tool round-trip; got ${JSON.stringify(response)} — the tool-reply may not have reached the worker (inference didn't resume)`,
);

console.log('\n=== Sync-callback path passed ===');

// === 5. Async callback (Promise-returning) ===
console.log('\n[5] Async-callback path: callback returns a Promise...');
let asyncCallCount = 0;
let asyncLastArgs = null;
const weatherToolAsync = m.Tool.fromFn(
  'get_weather',
  'Get the current weather for a city. Returns a short human-readable description.',
  {
    type: 'object',
    properties: {
      city: { type: 'string', description: 'City name in English' },
    },
    required: ['city'],
  },
  async (args) => {
    // Simulate a network call by deferring to the next tick.
    // This is the test of the async path — if core were still sync,
    // the Promise would never resolve while wasm holds the JS thread.
    await new Promise((resolve) => setTimeout(resolve, 5));
    asyncCallCount++;
    asyncLastArgs = args;
    return `Rainy in ${args.city ?? '?'}, 13 degrees Celsius.`;
  },
);

await chat.terminate();
const chatAsync = await m.Chat.create({
  modelPath,
  systemPrompt:
    'You are a helpful assistant. When the user asks about weather, use the get_weather tool. Then answer in one short sentence.',
  templateVariables: { enable_thinking: false },
  tools: [weatherToolAsync],
});

const t1 = performance.now();
const streamAsync = chatAsync.ask('What is the weather like in Oslo?');
const responseAsync = await streamAsync.completed();
const dt2 = ((performance.now() - t1) / 1000).toFixed(1);
console.log(`\n=== Async-callback response (${dt2} s) ===`);
console.log(responseAsync);
console.log(`\nasync tool was called ${asyncCallCount} time(s)`);
console.log(`last async args: ${JSON.stringify(asyncLastArgs)}`);

assert.ok(
  asyncCallCount >= 1,
  `expected the async JS callback to be invoked at least once; got ${asyncCallCount}`,
);
assert.ok(
  typeof asyncLastArgs === 'object' && asyncLastArgs !== null,
  `expected async callback args to be an object; got ${typeof asyncLastArgs}: ${JSON.stringify(asyncLastArgs)}`,
);
assert.ok(
  responseAsync.length > 0,
  `expected a non-empty final response after the async tool round-trip; got ${JSON.stringify(responseAsync)} — the tool-reply may not have reached the worker (inference didn't resume)`,
);

await chatAsync.terminate();

console.log('\n=== Tool-calling JS smoke test passed ===');
console.log('  Sync and async callbacks both dispatch through the Chat worker');
console.log('  tool-call / tool-reply RPC bridge. The async path proves the');
console.log('  wasm yields control to the JS event loop while parked at the');
console.log('  tool-dispatch await, letting the user-side Promise resolve');
console.log('  before inference resumes.');

process.exit(0);
