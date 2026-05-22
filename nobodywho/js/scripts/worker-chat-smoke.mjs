// WorkerChat smoke test for the JS binding under Node.
//
// Validates the full WorkerChat round-trip:
//   * `__nbw_spawn_worker` spawns a Node `worker_threads.Worker` from
//     a data URL with the polyfill preamble.
//   * The worker bootstrap polyfills `globalThis.postMessage` /
//     `globalThis.onmessage` from `parentPort`.
//   * `runInWorker` is auto-invoked via the `__nbw_node_worker` marker
//     in pre.js's postRun.
//   * Handshake (ready → load-model → model-loaded → create-chat →
//     chat-ready) completes.
//   * `WorkerChat.ask(...)` round-trips ask → token → ask-done.
//   * Optional sub-section: a tool callback survives the
//     `tool-call` / `tool-reply` RPC bridge across the worker boundary.
//
// Uses Qwen3-0.6B by default.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/worker-chat-smoke.mjs

import { readFileSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(here, '..', 'pkg-bundler');

const modelPath = process.argv[2]
  ?? '/nix/store/6i6yqpaz8ikxyi3lkmxj9zgwjdjsmwgi-Qwen_Qwen3-0.6B-Q4_K_M.gguf';
if (!existsSync(modelPath)) { console.error('missing model:', modelPath); process.exit(2); }

console.log('Loading wasm (main thread)...');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();

assert.equal(typeof m.WorkerChat, 'function', 'WorkerChat class must be exposed');
assert.equal(typeof m.WorkerChat.create, 'function', 'WorkerChat.create must be a function');
assert.equal(typeof globalThis.__nbw_spawn_worker, 'function',
  '__nbw_spawn_worker helper must be on globalThis (pre.js loaded?)');

const modelBytes = new Uint8Array(readFileSync(modelPath));
console.log(`Model bytes loaded: ${modelBytes.length} bytes`);

// === 1. Basic WorkerChat round-trip ===
console.log('\n[1] WorkerChat.create({modelBytes, ...}) → handshake → ask...');
const t0 = performance.now();
const wc = await m.WorkerChat.create({
  modelBytes,
  systemPrompt: 'You are a helpful assistant. Reply in one short sentence.',
  templateVariables: { enable_thinking: false },
});
const dtCreate = ((performance.now() - t0) / 1000).toFixed(1);
console.log(`    handshake complete in ${dtCreate} s`);

const stream = wc.ask('What is the capital of Denmark?');
const response = await stream.completed();
const dt = ((performance.now() - t0) / 1000).toFixed(1);
console.log(`\n=== Response (${dt} s total) ===`);
console.log(response);
assert.ok(
  response.toLowerCase().includes('copenhagen'),
  `expected response to mention Copenhagen; got: ${response}`,
);
console.log('    ✓ basic ask round-trip works');

wc.terminate();

// === 2. WorkerChat + tool callback (RPC bridge) ===
console.log('\n[2] WorkerChat({tools: [...]}) → tool-call RPC → tool-reply...');
let callCount = 0;
let lastArgs = null;
const weatherTool = m.Tool.fromFn(
  'get_weather',
  'Get the current weather for a city. Returns a short human-readable description.',
  {
    type: 'object',
    properties: { city: { type: 'string', description: 'City name in English' } },
    required: ['city'],
  },
  (args) => {
    callCount++;
    lastArgs = args;
    return `Sunny in ${args.city ?? '?'}, 21 degrees Celsius.`;
  },
);

const t1 = performance.now();
const wcWithTools = await m.WorkerChat.create({
  modelBytes,
  systemPrompt:
    'You are a helpful assistant. When the user asks about weather, use the get_weather tool. Then answer in one short sentence.',
  templateVariables: { enable_thinking: false },
  tools: [weatherTool],
});

const stream2 = wcWithTools.ask('What is the weather like in Copenhagen?');
const response2 = await stream2.completed();
const dt2 = ((performance.now() - t1) / 1000).toFixed(1);
console.log(`\n=== Response (${dt2} s) ===`);
console.log(response2);
console.log(`tool was called ${callCount} time(s); last args: ${JSON.stringify(lastArgs)}`);

assert.ok(
  callCount >= 1,
  `expected the tool callback to be invoked at least once; got ${callCount}`,
);

wcWithTools.terminate();

console.log('\n=== WorkerChat JS smoke test passed (Node worker_threads path) ===');
console.log('  __nbw_spawn_worker correctly spawned a Node worker_threads Worker.');
console.log('  Handshake + ask completed end-to-end.');
console.log('  tool-call / tool-reply RPC bridged across the worker boundary');
console.log('  → JS callback invoked from inside the worker-thread inference loop.');
