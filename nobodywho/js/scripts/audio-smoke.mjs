// Audio decoder smoke test for the JS binding.
//
// Win condition: bytes in each audio format (WAV, MP3, FLAC, Ogg/Vorbis)
// flow through `Audio.fromBytes(uint8)` → wasm boundary → mtmd's
// libc-fopen-based loader → miniaudio decoder → mtmd accepts the
// decoded PCM as audio chunks.
//
// We attempt full inference but DO NOT require it to succeed:
// downstream encoder support for any given audio mmproj on Emscripten
// is a separate concern (audio-LLM mmprojs use ops that may or may not
// work on wasm32). The win condition for THIS smoke is that miniaudio
// successfully decodes the bytes — which we detect by mtmd's
// `add_text: <|audio_bos|>` log line firing (mtmd only emits the
// audio-begin marker once it has accepted the decoded samples).
//
// Run after `bash js/scripts/build-pkg-emscripten.sh`:
//   PATH=/opt/homebrew/bin:$PATH node js/scripts/audio-smoke.mjs

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
// Capture mtmd's stdout (it writes "add_text: <|audio_bos|>" when it
// accepts decoded audio) so we can detect decoder success per format
// even if downstream inference crashes. Emscripten routes Module.print
// to stdout by default; we intercept and tee.
const mtmdLogLines = [];
const m = await createNobodyWhoModule({
  locateFile: (p) => join(pkgDir, p),
  print: (line) => { mtmdLogLines.push(line); process.stdout.write(line + '\n'); },
});
m.init();

const modelBytes = new Uint8Array(readFileSync(modelPath));
const mmprojBytes = new Uint8Array(readFileSync(mmprojPath));

const formats = [
  { ext: 'wav', desc: 'WAV (uncompressed PCM)' },
  { ext: 'mp3', desc: 'MP3' },
  { ext: 'flac', desc: 'FLAC' },
  { ext: 'ogg', desc: 'Ogg/Vorbis' },
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

  const chat = await m.Chat.create({
    modelBytes,
    mmprojBytes,
    systemPrompt: 'Transcribe the audio.',
    templateVariables: { enable_thinking: false },
  });

  // Audio.fromBytes is the JS-side test: bytes must transit the wasm
  // boundary without throwing (this is what the HEAPU8/passArray8ToWasm0
  // sed-patched helper covers). If this throws, byte-passing is broken.
  const audio = m.Audio.fromBytes(audioBytes);
  assert.equal(audio.__nbwKind, 'audio', 'Audio.fromBytes should produce a tagged object');

  // Snapshot mtmd log count before the ask — we count NEW occurrences
  // of <|audio_bos|> after the ask to confirm THIS format's audio was
  // accepted.
  const audioBosBefore = mtmdLogLines.filter(l => l.includes('<|audio_bos|>')).length;

  // Attempt inference. We don't require success here — the win is
  // mtmd seeing the audio. Catch any downstream encoder crash.
  let inferenceErr = null;
  try {
    await chat.ask([audio, 'Transcribe.']).completed();
  } catch (e) {
    inferenceErr = e.message ?? String(e);
  }

  const audioBosAfter = mtmdLogLines.filter(l => l.includes('<|audio_bos|>')).length;
  const decoderRan = audioBosAfter > audioBosBefore;

  if (decoderRan) {
    results[ext] = inferenceErr
      ? `decoder ok, encoder crashed: ${inferenceErr}`
      : 'full inference ok';
    console.log(`    ✓ decoder ran (mtmd emitted <|audio_bos|>)${inferenceErr ? '; encoder downstream crashed (separate issue)' : ''}`);
  } else {
    results[ext] = `decoder FAILED: ${inferenceErr ?? 'mtmd never accepted audio'}`;
    console.log(`    ✗ decoder did not run: ${inferenceErr ?? 'mtmd never accepted audio'}`);
  }

  await chat.terminate();
}

console.log('\n=== Audio decoder smoke summary ===');
for (const f of formats) console.log(`  ${f.ext.padEnd(5)} ${results[f.ext]}`);

const formatPassed = (f) => {
  const r = results[f.ext];
  return r === 'skipped' || r === 'full inference ok' || (typeof r === 'string' && r.startsWith('decoder ok'));
};
const allOk = formats.every(formatPassed);
const passed = formats.filter((f) => results[f.ext] === 'full inference ok' || (typeof results[f.ext] === 'string' && results[f.ext].startsWith('decoder ok'))).length;
const skipped = formats.filter((f) => results[f.ext] === 'skipped').length;

if (allOk) {
  console.log(`\n=== Audio decoder smoke passed (${passed}/${formats.length} decoded, ${skipped} skipped) ===`);
  console.log('  miniaudio decoders are linked + functional for each verified format.');
  console.log('  Downstream model-encoder support is a separate concern;');
  console.log('  see README "Outstanding" for Qwen3-ASR encoder status.');
  process.exit(0);
} else {
  console.error(`\n=== Audio decoder smoke FAILED ===`);
  process.exit(1);
}
