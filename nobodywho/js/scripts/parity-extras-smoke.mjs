// Smoke test for the last three Python-parity items:
//   * Audio.fromPath / Image.fromPath (Node-only ergonomic factories)
//   * cosineSimilarity helper
//   * Chat.reset({systemPrompt?, tools?}) — atomic combined reset
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/parity-extras-smoke.mjs

import { readFileSync, existsSync, writeFileSync, mkdirSync } from 'node:fs';
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

// === [1] cosineSimilarity ===
console.log('\n[1] cosineSimilarity helper...');
const sim_same = m.cosineSimilarity(new Float32Array([1, 0, 0]), new Float32Array([1, 0, 0]));
console.log(`    identical vectors: ${sim_same}`);
assert.equal(sim_same.toFixed(6), '1.000000');

const sim_orth = m.cosineSimilarity(new Float32Array([1, 0, 0]), new Float32Array([0, 1, 0]));
console.log(`    orthogonal vectors: ${sim_orth}`);
assert.equal(sim_orth.toFixed(6), '0.000000');

const sim_opp = m.cosineSimilarity([1, 0], [-1, 0]);  // plain array also works
console.log(`    opposite vectors (plain array): ${sim_opp}`);
assert.equal(sim_opp.toFixed(6), '-1.000000');

try {
  m.cosineSimilarity([1, 2, 3], [1, 2]);
  assert.fail('expected length mismatch to throw');
} catch (e) {
  console.log(`    length mismatch throws: ${e.message.slice(0, 60)}`);
}
console.log('    ✓ cosineSimilarity OK');

// === [2] Audio.fromPath ===
console.log('\n[2] Audio.fromPath (Node-only)...');
// Create a tiny valid WAV file in /tmp to read.
const tmpDir = '/tmp/parity-smoke';
mkdirSync(tmpDir, { recursive: true });
const wavPath = join(tmpDir, 'silence.wav');
// 44-byte WAV header + 0 PCM samples = 44 bytes total (smallest valid WAV).
const wavHeader = Buffer.from([
  0x52, 0x49, 0x46, 0x46, 0x24, 0x00, 0x00, 0x00, 0x57, 0x41, 0x56, 0x45,
  0x66, 0x6d, 0x74, 0x20, 0x10, 0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00,
  0x44, 0xac, 0x00, 0x00, 0x88, 0x58, 0x01, 0x00, 0x02, 0x00, 0x10, 0x00,
  0x64, 0x61, 0x74, 0x61, 0x00, 0x00, 0x00, 0x00,
]);
writeFileSync(wavPath, wavHeader);
const audioFromPath = await m.Audio.fromPath(wavPath);
console.log(`    audio: kind=${audioFromPath.__nbwKind}, bytes.length=${audioFromPath.bytes.length}`);
assert.equal(audioFromPath.__nbwKind, 'audio');
assert.equal(audioFromPath.bytes.length, 44);
// Same as fromBytes for parity:
const audioFromBytes = m.Audio.fromBytes(new Uint8Array(readFileSync(wavPath)));
assert.equal(audioFromPath.bytes.length, audioFromBytes.bytes.length);
console.log('    ✓ Audio.fromPath returns same shape as fromBytes');

// === [3] Image.fromPath ===
console.log('\n[3] Image.fromPath (Node-only)...');
// Tiny valid PNG (1x1 transparent) — 67 bytes.
const pngPath = join(tmpDir, 'pixel.png');
const pngBytes = Buffer.from([
  0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d,
  0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01,
  0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00,
  0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
  0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
  0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
]);
writeFileSync(pngPath, pngBytes);
const imgFromPath = await m.Image.fromPath(pngPath);
console.log(`    image: kind=${imgFromPath.__nbwKind}, bytes.length=${imgFromPath.bytes.length}`);
assert.equal(imgFromPath.__nbwKind, 'image');
assert.equal(imgFromPath.bytes.length, pngBytes.length);
console.log('    ✓ Image.fromPath works');

// === [4] Chat.reset(opts) — combined atomic reset ===
console.log('\n[4] Chat.reset({systemPrompt, tools})...');
const chat = await m.Chat.create({
  modelPath,
  systemPrompt: 'Initial system.',
  templateVariables: { enable_thinking: false },
});
// Establish baseline state: do an ask, confirm history non-empty.
await chat.ask('Say "ack".').completed();
const before = await chat.getChatHistory();
console.log(`    history before reset: ${before.length} entries`);
const sysBefore = await chat.getSystemPrompt();
console.log(`    system before reset: ${JSON.stringify(sysBefore)}`);

// Atomic reset: clear history + replace system prompt.
await chat.reset({ systemPrompt: 'Reset persona — reply briefly.' });
const after = await chat.getChatHistory();
const sysAfter = await chat.getSystemPrompt();
console.log(`    history after reset: ${after.length} entries`);
console.log(`    system after reset: ${JSON.stringify(sysAfter)}`);
assert.equal(after.length, 0, 'history should be empty after reset');
assert.equal(sysAfter, 'Reset persona — reply briefly.');

// Verify the reset chat is still usable.
const reply = await chat.ask('Say "ok".').completed();
console.log(`    ask after reset: ${JSON.stringify(reply).slice(0, 40)}`);
assert.ok(reply.length > 0);

// Test reset with no opts (clears system prompt + tools too).
await chat.reset();
const sysNull = await chat.getSystemPrompt();
console.log(`    after empty reset(), system: ${JSON.stringify(sysNull)}`);
assert.equal(sysNull, null, 'reset() with no opts clears system prompt');

await chat.terminate();
console.log('    ✓ reset round-trip works');

console.log('\n=== parity-extras-smoke passed ===');
process.exit(0);
