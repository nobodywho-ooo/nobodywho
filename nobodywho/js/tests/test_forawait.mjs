// Test: verify `for await (const tok of stream)` works after the
// Symbol.asyncIterator shim landed in pre.js.
//
// Usage: node test_forawait.mjs <model.gguf>
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: test_forawait.mjs <model.gguf>');
  process.exit(2);
}

const chat = await m.Chat.create({
  modelPath,
  systemPrompt: 'You are a helpful assistant.',
  templateVariables: { enable_thinking: false },
});

const stream = chat.ask('Say hello in one short sentence.');

// Sanity-check the protocol attachment up front
const proto = Object.getPrototypeOf(stream);
const hasIter = typeof proto[Symbol.asyncIterator] === 'function';
console.log(`[check] TokenStream prototype has [Symbol.asyncIterator]: ${hasIter}`);
if (!hasIter) {
  console.error('FAIL: shim did not attach');
  chat.free();
  process.exit(1);
}

// Now actually use the for-await form
let count = 0;
let text = '';
for await (const tok of stream) {
  count++;
  text += tok;
  if (count <= 5) console.log(`[tok ${count}] ${JSON.stringify(tok)}`);
}
console.log(`---`);
console.log(`total tokens iterated: ${count}`);
console.log(`accumulated text:      ${JSON.stringify(text)}`);

chat.free();
if (count <= 1) {
  console.error('FAIL: expected multiple tokens via for-await');
  process.exit(1);
}
console.log('PASS');
