import { readFileSync } from 'node:fs';
import { Model, Encoder } from './setup.mjs';

const model = await Model.loadBytes(new Uint8Array(readFileSync(process.argv[2])));
const encoder = new Encoder(model, 2048);

const texts = [
  'The quick brown fox jumps over the lazy dog.',
  'A fast brown fox leaps over the sleepy dog.',
  'The weather is sunny today.',
  'Machine learning is a subset of artificial intelligence.',
  'Deep learning uses neural networks with many layers.',
];

const embeddings = [];
for (const text of texts) embeddings.push(await encoder.encode(text));

// Python has nobodywho.cosine_similarity; the JS binding doesn't, so inline it.
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
