// Audio decoder test for the JS binding.
//
// Win condition: bytes in each audio format (WAV, MP3, FLAC) flow
// through `Audio.fromBytes(uint8)` → wasm boundary → mtmd's
// libc-fopen-based loader → miniaudio decoder → mtmd accepts the
// decoded PCM → audio mmproj encoder → LLM produces a transcript.
//
// Verified end-to-end against Qwen3-ASR. On Emscripten, mtmd audio
// preprocessing runs under real pthreads now — the earlier n_threads=1
// workaround (nobodywho-ooo/llama.cpp fork) is gone; the llama.cpp
// submodule is stock ggml-org.
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/tests/test_audio.mjs

import { readFileSync, existsSync } from 'node:fs';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { strict as assert } from 'node:assert';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = resolve(here, '..', 'pkg-bundler');

const modelPath = process.argv[2] ?? '/tmp/audio-test/Qwen3-ASR-0.6B-Q8_0.gguf';
const mmprojPath = process.argv[3] ?? '/tmp/audio-test/mmproj-Qwen3-ASR-0.6B-Q8_0.gguf';
const audioDir = process.argv[4] ?? '/tmp/audio-test';

for (const [label, p] of [['model', modelPath], ['mmproj', mmprojPath]]) {
  if (!existsSync(p)) { console.error(`missing ${label}: ${p}`); process.exit(2); }
}

console.log('Loading wasm...');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const formats = [
  { ext: 'wav', desc: 'WAV (uncompressed PCM)' },
  { ext: 'mp3', desc: 'MP3' },
  { ext: 'flac', desc: 'FLAC' },
];

const results = {};

for (const { ext, desc } of formats) {
  const audioPath = join(audioDir, `sound.${ext}`);
  if (!existsSync(audioPath)) {
    console.log(`\n[skip ${ext.toUpperCase()}] no test file at ${audioPath}`);
    results[ext] = 'skipped';
    continue;
  }
  console.log(`\n[${ext.toUpperCase()}] ${desc} — ${audioPath}`);
  const audioBytes = new Uint8Array(readFileSync(audioPath));
  console.log(`    audio: ${audioBytes.length} bytes`);

  // Audio.fromBytes is the JS-side test: bytes must transit the wasm
  // boundary without throwing (this is what the HEAPU8/passArray8ToWasm0
  // sed-patched helper covers).
  const audio = m.Audio.fromBytes(audioBytes);
  assert.equal(audio.__nbwKind, 'audio', 'Audio.fromBytes should produce a tagged object');
  console.log(`    Audio.fromBytes ✓ (tagged object, ${audio.bytes.length} bytes)`);

  // Try a full inference. We don't require it to succeed — the win is
  // mtmd accepting the decoded audio (visible as the `<|audio_bos|>`
  // marker in stdout). The downstream encoder may crash for audio
  // mmprojs that use ops not in our wasm build.
  const chat = await m.Chat.create({
    modelPath,
    mmprojPath,
    systemPrompt: 'Transcribe the audio.',
    templateVariables: { enable_thinking: false },
  });

  let inferenceErr = null;
  let response = null;
  let chunkCount = 0;
  try {
    // for-await streams tokens via the per-token hook (same path as
    // text/vision). Multi-chunk return confirms streaming engages on
    // audio prompts too.
    response = '';
    for await (const tok of chat.ask([audio, 'Transcribe.'])) {
      chunkCount++;
      response += tok;
    }
  } catch (e) {
    inferenceErr = e.message ?? String(e);
    response = null;
  }

  if (response) {
    results[ext] = { state: 'full-ok', response: response.slice(0, 200), chunkCount };
    console.log(`    ✓ full inference: ${chunkCount} chunks, ${JSON.stringify(response.slice(0, 100))}`);
  } else {
    // Audio.fromBytes worked and we sent it to the worker. The
    // downstream crash (if any) is documented separately. The
    // decoder claim ("WAV/MP3/FLAC decoders are linked + functional")
    // is verified by Audio.fromBytes accepting bytes that the worker
    // can structured-clone over to mtmd.
    results[ext] = { state: 'decoder-ok', err: inferenceErr };
    console.log(`    ✓ Audio.fromBytes passed bytes through wasm; downstream inference error (separate issue): ${inferenceErr}`);
  }

  chat.free();
}

console.log('\n=== Audio decoder summary ===');
for (const f of formats) {
  const r = results[f.ext];
  const s = typeof r === 'string' ? r : r.state;
  console.log(`  ${f.ext.padEnd(5)} ${s}`);
}

const fullPassed = formats.filter((f) => typeof results[f.ext] === 'object' && results[f.ext].state === 'full-ok').length;
const decoderPassed = formats.filter((f) => typeof results[f.ext] === 'object' && results[f.ext].state === 'decoder-ok').length;
const skipped = formats.filter((f) => results[f.ext] === 'skipped').length;

// The old `allOk` counted all-skipped AND inference-threw-but-caught as a
// pass, so it could exit 0 with zero transcripts. Audio inference is verified
// working end-to-end against the default Qwen3-ASR model, so require at least
// one real transcript and refuse to pass when every format was skipped.
if (skipped === formats.length) {
  console.error(`\n=== Audio FAILED: every format skipped — no audio test files found ===`);
  process.exit(1);
}
if (fullPassed < 1) {
  console.error(`\n=== Audio FAILED: no format produced a transcript ===`);
  console.error(`  full-ok=${fullPassed} decoder-ok=${decoderPassed} skipped=${skipped}`);
  console.error(`  A decoder-only result means the encode/inference path regressed`);
  console.error(`  (it is known to work with the default Qwen3-ASR model) — not a pass.`);
  process.exit(1);
}
console.log(`\n=== Audio passed ===`);
console.log(`  ${fullPassed}/${formats.length} full transcript(s), ${decoderPassed} decoder-only, ${skipped} skipped`);
process.exit(0);
