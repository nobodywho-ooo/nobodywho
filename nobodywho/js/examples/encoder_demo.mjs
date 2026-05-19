import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();

const model = await m.Model.loadBytes(new Uint8Array(readFileSync(process.argv[2])));
const encoder = new m.Encoder(model, 2048);

const texts = [
  'The quick brown fox jumps over the lazy dog.',
  'A fast brown fox leaps over the sleepy dog.',
  'The weather is sunny today.',
  'Machine learning is a subset of artificial intelligence.',
  'Deep learning uses neural networks with many layers.',
];

const embeddings = [];
for (const text of texts) embeddings.push(await encoder.encode(text));

const cosine = (a, b) => {
  let dot = 0, na = 0, nb = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    na += a[i] * a[i];
    nb += b[i] * b[i];
  }
  return dot / Math.sqrt(na * nb);
};

for (let i = 0; i < texts.length; i++) {
  for (let j = i + 1; j < texts.length; j++) {
    console.log(`${i + 1} vs ${j + 1}: ${cosine(embeddings[i], embeddings[j]).toFixed(3)}`);
  }
}
