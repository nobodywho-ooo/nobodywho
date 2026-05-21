// Isolated inference benchmark. Loads model once, times only the
// streaming-generation loop, reports tokens/sec. Run as:
//   node bench-chat.mjs <model.gguf> [trials] [max_tokens]
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(here, '..', 'pkg-bundler');
const modelPath = process.argv[2];
const trials = Number(process.argv[3] ?? 5);
const maxTokens = Number(process.argv[4] ?? 100);
if (!modelPath) { console.error('usage: bench-chat.mjs <model.gguf> [trials=5] [max_tokens=100]'); process.exit(2); }

const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();

const loadStart = performance.now();
const bytes = new Uint8Array(readFileSync(modelPath));
const model = await m.Model.loadBytes(bytes);
const loadMs = performance.now() - loadStart;
console.log(`load: ${loadMs.toFixed(0)} ms (${(bytes.byteLength / 1e6).toFixed(1)} MB model)`);

const prompt = 'Write a short paragraph describing the city of Copenhagen.';
const results = [];
for (let t = 0; t < trials; t++) {
  const chat = new m.Chat(model, {
    systemPrompt: 'You are a helpful assistant',
    templateVariables: { enable_thinking: false },
  });
  let nTok = 0;
  const start = performance.now();
  await chat.askStreaming(prompt, () => {
    nTok++;
    if (nTok >= maxTokens) throw new Error('stop');
  }).catch(() => {});
  const ms = performance.now() - start;
  const tps = (nTok / ms) * 1000;
  results.push({ ms, nTok, tps });
  console.log(`trial ${t + 1}: ${nTok} tokens in ${ms.toFixed(0)} ms → ${tps.toFixed(2)} tok/s`);
}

const tpss = results.map(r => r.tps).sort((a, b) => a - b);
const median = tpss[Math.floor(tpss.length / 2)];
const min = tpss[0], max = tpss[tpss.length - 1];
console.log(`\nsummary: median ${median.toFixed(2)} tok/s (min ${min.toFixed(2)}, max ${max.toFixed(2)})`);
