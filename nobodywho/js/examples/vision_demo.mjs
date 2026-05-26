import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();

const modelPath = process.argv[2];
const mmprojPath = process.argv[3];
const imagePath = process.argv[4];
if (!modelPath || !mmprojPath || !imagePath) {
  console.error('usage: node vision_demo.mjs <model.gguf> <mmproj.gguf> <image.png>');
  process.exit(2);
}

const modelBytes = new Uint8Array(readFileSync(modelPath));
const mmprojBytes = new Uint8Array(readFileSync(mmprojPath));
const imgBytes = new Uint8Array(readFileSync(imagePath));

const chat = await m.Chat.create({
  modelBytes,
  mmprojBytes,
  systemPrompt: 'You are a helpful assistant. Be brief.',
  templateVariables: { enable_thinking: false },
  contextSize: 4096,
});

const img = m.Image.fromBytes(imgBytes);
let response = '';
let chunks = 0;
for await (const tok of chat.ask([
  'What animal is in this image? One word, lowercase.',
  img,
])) {
  chunks++;
  response += tok;
}

console.log(`Vision response: ${response.trim()}`);
console.log(`Chunks streamed: ${chunks}`);

await chat.terminate();
