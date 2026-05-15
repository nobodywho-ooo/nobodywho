// Simple embeddings demo: encode a few texts, print pairwise cosine
// similarities. Mirrors python/examples/encoder_demo.py.
// Usage: node js/examples/encoder_demo.mjs /path/to/embeddings.gguf

import { readFileSync } from 'node:fs';
import { Model, Encoder } from './setup.mjs';

const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: node encoder_demo.mjs /path/to/embeddings.gguf');
  process.exit(1);
}

console.log(`Loading embeddings model: ${modelPath}`);
const model = await Model.loadBytes(new Uint8Array(readFileSync(modelPath)));
const encoder = new Encoder(model, 2048);

const texts = [
  'The quick brown fox jumps over the lazy dog.',
  'A fast brown fox leaps over the sleepy dog.',
  'The weather is sunny today.',
  'Machine learning is a subset of artificial intelligence.',
  'Deep learning uses neural networks with many layers.',
];

console.log('\nGenerating embeddings...');
const embeddings = [];
for (const [i, text] of texts.entries()) {
  console.log(`  ${i + 1}. ${text}`);
  embeddings.push(await encoder.encode(text));
}

console.log(`\nEmbedding dimension: ${embeddings[0].length}`);
const first8 = Array.from(embeddings[0].slice(0, 8)).map((v) => v.toFixed(4));
console.log(`first 8: [${first8.join(', ')}]`);

function cosineSimilarity(a, b) {
  let dot = 0, na = 0, nb = 0;
  for (let i = 0; i < a.length; i++) {
    dot += a[i] * b[i];
    na += a[i] * a[i];
    nb += b[i] * b[i];
  }
  return dot / Math.sqrt(na * nb);
}

console.log('\nPairwise cosine similarities:');
for (let i = 0; i < texts.length; i++) {
  for (let j = i + 1; j < texts.length; j++) {
    const sim = cosineSimilarity(embeddings[i], embeddings[j]);
    console.log(`  Text ${i + 1} vs Text ${j + 1}: ${sim.toFixed(3)}`);
  }
}

console.log('\nDone!');
