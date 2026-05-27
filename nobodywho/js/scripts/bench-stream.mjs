// Streaming-latency benchmark for the worker-backed Chat.
//
// For each trial, creates a Chat (spawning a fresh worker that loads the
// model) and times:
//   - TTFT   = time from chat.ask() to first token arriving at main
//   - TTLT   = time to last token (just before ask-done)
//   - total  = wall time from ask() to completed() resolving
//   - tokens = number of times stream.next() yielded
//   - tok/s  = effective rate (chars/4 / total_time as proxy when only one
//              "token" arrives, which is what happens without the hook)
//
// Usage:
//   node bench-stream.mjs <model.gguf> [trials=3] [max_tokens=80]
//
// Wasm stderr (model-load tensor noise) is suppressed by default — set
// NBW_SHOW_WASM_STDERR=1 to see it.
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
const pkgDir = join(here, '..', 'pkg-bundler');
const modelPath = process.argv[2];
const trials = Number(process.argv[3] ?? 3);
const maxTokens = Number(process.argv[4] ?? 80);
if (!modelPath) {
  console.error('usage: bench-stream.mjs <model.gguf> [trials=3] [max_tokens=80]');
  process.exit(2);
}

// Silence the noisy [wasm stderr] tensor-load lines unless explicitly asked.
// Worker process.stderr.write calls land here on the main thread.
if (!process.env.NBW_SHOW_WASM_STDERR) {
  const realWrite = process.stderr.write.bind(process.stderr);
  process.stderr.write = (chunk, ...rest) => {
    const s = typeof chunk === 'string' ? chunk : chunk?.toString?.() ?? '';
    if (s.startsWith('[wasm stderr]')) return true;
    return realWrite(chunk, ...rest);
  };
}

const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

console.log(`trials=${trials}  max_tokens=${maxTokens}`);
console.log('');

const prompt =
  'Write three paragraphs describing the city of Copenhagen, its history, ' +
  'culture, and modern character. Be specific and detailed.';

const results = [];
for (let t = 0; t < trials; t++) {
  process.stdout.write(`trial ${t + 1}/${trials}: loading model into worker… `);
  const chat = await m.Chat.create({
    modelPath,
    systemPrompt: 'You are a helpful assistant.',
    templateVariables: { enable_thinking: false },
  });
  process.stdout.write('ready. inferring… ');

  const askStart = performance.now();
  const stream = chat.ask(prompt);

  let ttftMs = null;
  let ttltMs = null;
  let count = 0;
  let totalChars = 0;
  let firstTokenPreview = '';

  while (true) {
    const { value, done } = await stream.next();
    if (done) break;
    const now = performance.now();
    if (ttftMs === null) {
      ttftMs = now - askStart;
      firstTokenPreview = value.slice(0, 40).replace(/\n/g, '\\n');
    }
    ttltMs = now - askStart;
    count++;
    totalChars += value.length;
    if (count >= maxTokens) break;
  }

  // Drain to completion so totalMs is honest wall-time-to-EOS.
  let totalMs;
  try {
    await stream.completed();
  } catch {}
  totalMs = performance.now() - askStart;

  await chat.terminate();

  // tok/s: prefer real per-token rate when we have >1 token. Otherwise
  // fall back to chars/4 as a rough token-count proxy over total time —
  // useful for comparing "buffered into one chunk" vs "real streaming".
  const streamMs = (ttltMs ?? 0) - (ttftMs ?? 0);
  let tps, tpsNote;
  if (count > 1 && streamMs > 0) {
    tps = (count / streamMs) * 1000;
    tpsNote = 'stream phase';
  } else {
    tps = (totalChars / 4 / totalMs) * 1000;
    tpsNote = 'chars/4 ÷ total';
  }

  results.push({ ttftMs, ttltMs, totalMs, count, totalChars, tps });
  process.stdout.write('done\n');
  console.log(
    `  ttft=${(ttftMs ?? 0).toFixed(0)}ms  ` +
    `ttlt=${(ttltMs ?? 0).toFixed(0)}ms  ` +
    `total=${totalMs.toFixed(0)}ms  ` +
    `next()=${count}  chars=${totalChars}  ` +
    `~tok/s=${tps.toFixed(2)} (${tpsNote})`,
  );
  console.log(`  first chunk: "${firstTokenPreview}${firstTokenPreview.length === 40 ? '…' : ''}"`);
  console.log('');
}

function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

const ttftMed = median(results.map(r => r.ttftMs ?? 0));
const ttltMed = median(results.map(r => r.ttltMs ?? 0));
const totMed = median(results.map(r => r.totalMs));
const cntMed = median(results.map(r => r.count));
const tpsMed = median(results.map(r => r.tps));

console.log('summary (medians)');
console.log(`  TTFT:    ${ttftMed.toFixed(0)} ms`);
console.log(`  TTLT:    ${ttltMed.toFixed(0)} ms`);
console.log(`  total:   ${totMed.toFixed(0)} ms`);
console.log(`  next():  ${cntMed} chunks`);
console.log(`  ~tok/s:  ${tpsMed.toFixed(2)}`);
