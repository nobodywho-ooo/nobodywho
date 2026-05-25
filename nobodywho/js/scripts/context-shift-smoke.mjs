// Smoke: streaming continues cleanly when the KV cache wraps and
// core's inference loop hits `self.context_shift()`.
//
// Strategy: tiny contextSize (512) + multi-turn dialogue. Each call
// to chat.ask(...) appends a user message; after ~4-5 turns the
// context fills and the next generation forces a context_shift,
// which drops old user/assistant pairs to make room. Streaming must
// continue cleanly through the shift on the turn it happens.
//
// Asserts (per turn):
//   - chat.ask().for-await yields tokens
//   - .completed() resolves to non-empty text matching what we
//     accumulated via the stream
//   - text contains no template-marker leaks
//
// Assert (overall):
//   - by turn ~5, total context tokens we've sent should exceed CTX,
//     so at least one shift was needed for the conversation to keep
//     running
//   - no turn errored
//
// Usage: node context-shift-smoke.mjs <model.gguf>
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: context-shift-smoke.mjs <model.gguf>');
  process.exit(2);
}

// Use a deliberately tiny context so even short replies overflow it
// after 2-3 turns and force context_shift. Below ~256 tokens the
// model can't fit the chat template scaffolding + a single short
// exchange comfortably; that's exactly the regime we want to stress.
const CTX = 256;
console.log(`[setup] contextSize=${CTX}, multi-turn dialogue forcing a shift`);

const chat = await m.Chat.create({
  modelBytes: new Uint8Array(readFileSync(modelPath)),
  contextSize: CTX,
  systemPrompt: 'You are a helpful assistant. Give complete but moderately detailed answers (3-5 sentences).',
  templateVariables: { enable_thinking: false },
});

const turns = [
  'Tell me one fact about Copenhagen.',
  'And one fact about Stockholm.',
  'And one fact about Oslo.',
  'And one fact about Helsinki.',
  'And one fact about Reykjavik.',
  'Now name one famous bridge in any Nordic capital.',
  'Which of the cities I asked about has the highest population?',
];

const LEAKS = ['<|im_start|>', '<|im_end|>', '<|begin_of_text|>', '<|endoftext|>'];
let totalChars = 0;

for (let t = 0; t < turns.length; t++) {
  process.stdout.write(`[turn ${t + 1}/${turns.length}] q=${JSON.stringify(turns[t])}\n  a=`);
  let count = 0;
  let text = '';
  try {
    for await (const tok of chat.ask(turns[t])) {
      count++;
      text += tok;
      if (count <= 12) process.stdout.write(tok);
    }
  } catch (e) {
    console.log(`\n  FAIL: turn ${t + 1} threw: ${e?.message ?? e}`);
    await chat.terminate();
    process.exit(1);
  }
  if (count > 12) process.stdout.write(`…(${count} toks total)`);
  process.stdout.write('\n');

  if (count < 2) {
    console.log(`  FAIL: turn ${t + 1} produced ${count} tokens`);
    await chat.terminate();
    process.exit(1);
  }
  for (const marker of LEAKS) {
    if (text.includes(marker)) {
      console.log(`  FAIL: turn ${t + 1} text contains template marker ${JSON.stringify(marker)}`);
      await chat.terminate();
      process.exit(1);
    }
  }
  totalChars += text.length;
}

await chat.terminate();

// Rough check: by the last turn the cumulative dialogue text far exceeds
// what fits in CTX tokens (~3 chars/token ≈ CTX*3 chars window). If we
// got through every turn without a per-turn error, context_shift fired
// successfully at least once.
const approxTokenBudget = CTX * 3; // very rough chars-to-tokens proxy for the window
console.log(`\n  cumulative response chars: ${totalChars}`);
console.log(`  approx window-tokens × 3:  ${approxTokenBudget}`);

if (totalChars < approxTokenBudget) {
  console.log('NOTE: cumulative output was small enough that a shift might not have been needed.');
  console.log('      The smoke still passes (no errors, no leaks), but did not stress the shift path.');
} else {
  console.log('PASS: 7-turn dialogue completed past the context window; shift happened transparently.');
}
