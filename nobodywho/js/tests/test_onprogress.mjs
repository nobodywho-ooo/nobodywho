// Test for the `onProgress` download callback on URL (streaming) loads.
//
// Serves a GGUF over a local HTTP server and loads it via
// `Model.load({ modelUrl, onProgress })`, asserting the callback fires with
// monotonically-increasing (loaded, total, kind) and that the streamed model
// then actually works (Encoder.encode round-trip).
//
// Why this works under Node: the browser Cache API (`caches`) is absent in
// Node, so the loader's `open_model_cache()` returns None and the plain
// fetch()+tee() streaming path runs — which is exactly the path that fires
// onProgress. (NODEFS `modelPath` loads read from disk without streaming, so
// they never fire it.)
//
//   node js/tests/test_onprogress.mjs /path/to/embedding.gguf
//
// Exit 0 + "=== onProgress passed ===" on success.

import { existsSync, statSync, createReadStream } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createServer } from 'node:http';
import { strict as assert } from 'node:assert';

const here = fileURLToPath(new URL('.', import.meta.url));
const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: node test_onprogress.mjs <path-to-embedding.gguf>');
  process.exit(2);
}
if (!existsSync(modelPath)) {
  console.error(`model not found: ${modelPath}`);
  process.exit(2);
}

const loaderPath = [
  resolve(here, '..', 'pkg-bundler', 'nobodywho_js.js'),
  resolve(here, '..', '..', 'target', 'wasm32-unknown-emscripten', 'release', 'nobodywho_js.js'),
  resolve(here, '..', '..', 'target', 'wasm32-unknown-emscripten', 'debug', 'nobodywho_js.js'),
].find(existsSync);
if (!loaderPath) {
  console.error('Emscripten loader not found — build first: bash js/scripts/build-pkg-emscripten.sh');
  process.exit(2);
}
console.log(`loading: ${loaderPath}`);

// --- Serve the GGUF over HTTP with a real Content-Length, so the loader
//     takes the size-known (single-copy) path and `total` is non-zero. ---
const size = statSync(modelPath).size;
const server = createServer((req, res) => {
  res.writeHead(200, {
    'content-type': 'application/octet-stream',
    'content-length': String(size),
  });
  createReadStream(modelPath).pipe(res);
});
await new Promise((r) => server.listen(0, '127.0.0.1', r));
const { port } = server.address();
const modelUrl = `http://127.0.0.1:${port}/model.gguf`;
console.log(`serving ${modelPath} (${size} bytes) at ${modelUrl}`);

let failure = null;
try {
  const { default: createNobodyWhoModule } = await import(loaderPath);
  const module = await createNobodyWhoModule({
    locateFile: (path) => resolve(loaderPath, '..', path),
  });

  // Record every progress tick.
  const ticks = [];
  const onProgress = (loaded, total, kind) => {
    ticks.push({ loaded, total, kind });
  };

  console.log('Model.load({ modelUrl, onProgress })...');
  const model = await module.Model.load({ modelUrl, onProgress });

  // --- Assertions on the progress stream ---
  assert.ok(ticks.length > 0, 'onProgress was never called on a URL/streaming load');
  for (const [i, t] of ticks.entries()) {
    assert.equal(typeof t.loaded, 'number', `tick ${i}: loaded must be a number`);
    assert.equal(typeof t.total, 'number', `tick ${i}: total must be a number`);
    assert.equal(t.kind, 'model', `tick ${i}: kind must be 'model' (got ${t.kind})`);
    assert.equal(t.total, size, `tick ${i}: total must equal Content-Length ${size} (got ${t.total})`);
  }
  // Monotonic non-decreasing loaded, ending exactly at the file size.
  for (let i = 1; i < ticks.length; i++) {
    assert.ok(ticks[i].loaded >= ticks[i - 1].loaded, `loaded went backwards at tick ${i}`);
  }
  assert.equal(ticks.at(-1).loaded, size, `final loaded must equal ${size}, got ${ticks.at(-1).loaded}`);
  console.log(`onProgress fired ${ticks.length}x; final ${ticks.at(-1).loaded}/${ticks.at(-1).total} (kind=model)`);

  // --- Confirm the streamed model actually loaded (end-to-end). ---
  console.log('new Encoder + encode("test")...');
  const encoder = new module.Encoder(model, 2048);
  const vec = await encoder.encode('test');
  assert.ok(vec.length > 0, 'embedding is empty');
  assert.ok(
    Array.from(vec.slice(0, 8)).every((x) => Number.isFinite(x)),
    'first 8 embedding values must be finite',
  );
  console.log(`embedding dimension: ${vec.length}`);

  console.log('\n=== onProgress passed ===');
} catch (e) {
  failure = e;
} finally {
  server.close();
}
if (failure) {
  console.error('\nFAILED:', failure?.stack || failure);
  process.exit(1);
}
