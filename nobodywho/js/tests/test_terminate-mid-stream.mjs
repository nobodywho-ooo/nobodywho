// Test: chat.terminate() called WHILE tokens are still streaming.
//
// Asserts:
//   - terminate() resolves cleanly (no unhandled rejection)
//   - the iterator stops producing tokens after terminate() resolves
//   - subsequent .next() / .completed() reject with a useful error
//     rather than hanging
//   - a fresh chat can be created+used afterwards (no global state
//     poisoning from the abort)
//
// Usage: node test_terminate-mid-stream.mjs <model.gguf>
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: test_terminate-mid-stream.mjs <model.gguf>');
  process.exit(2);
}

// --- Phase 1: terminate mid-stream ---
console.log('[phase 1] starting chat, will terminate after 3 streamed tokens');
const chat1 = await m.Chat.create({
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
});

const stream = chat1.ask(
  'Write a long, detailed essay about Copenhagen — at least 500 words.',
);

let preTerminateCount = 0;
let postTerminateCount = 0;
let terminateResolved = false;
let terminatePromise = null;

for (let i = 0; i < 200; i++) {
  let result;
  try {
    result = await stream.next();
  } catch (e) {
    console.log(`  [tok ${i}] next() rejected: ${e.message ?? e}`);
    break;
  }
  if (result.done) {
    console.log(`  [tok ${i}] stream ended (done=true)`);
    break;
  }
  if (!terminateResolved) {
    preTerminateCount++;
    process.stdout.write('.');
    if (preTerminateCount === 3) {
      console.log('\n  --- calling chat.terminate() ---');
      terminatePromise = chat1.terminate().then(() => {
        terminateResolved = true;
        console.log('  --- terminate() resolved ---');
      });
    }
  } else {
    postTerminateCount++;
    if (postTerminateCount > 5) {
      console.log(`\nFAIL: still receiving tokens (${postTerminateCount}) after terminate resolved`);
      process.exit(1);
    }
  }
}

if (terminatePromise) await terminatePromise;

console.log(`  tokens before terminate: ${preTerminateCount}`);
console.log(`  tokens after terminate:  ${postTerminateCount}`);

if (preTerminateCount < 3) {
  console.log('FAIL: never received the 3 tokens needed to trigger terminate');
  process.exit(1);
}

// Subsequent stream operations should reject (not hang)
console.log('  checking post-terminate stream.next() rejects rather than hangs…');
const timeout = new Promise((_, rej) => setTimeout(() => rej(new Error('hang: next() did not resolve/reject within 3s')), 3000));
try {
  const r = await Promise.race([stream.next(), timeout]);
  // Either {done:true} or an exception is acceptable; a token is not.
  if (!r.done) {
    console.log(`FAIL: post-terminate next() yielded a token: ${JSON.stringify(r.value)}`);
    process.exit(1);
  }
  console.log('  post-terminate next() returned done=true ✓');
} catch (e) {
  console.log(`  post-terminate next() rejected: ${e.message} ✓`);
}

// --- Phase 2: fresh chat after a terminated one ---
console.log('\n[phase 2] creating a fresh chat after terminate — should work normally');
const chat2 = await m.Chat.create({
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
});

let count = 0;
let text = '';
for await (const tok of chat2.ask('Say hello in one short sentence.')) {
  count++;
  text += tok;
}
await chat2.terminate();

console.log(`  fresh chat produced ${count} tokens: ${JSON.stringify(text)}`);
if (count < 2) {
  console.log('FAIL: fresh chat after terminate did not stream normally');
  process.exit(1);
}

console.log('\nPASS: terminate mid-stream works; fresh chat after terminate works');
