import { readFileSync } from 'node:fs';
import { Model, CrossEncoder } from './setup.mjs';

const model = await Model.loadBytes(new Uint8Array(readFileSync(process.argv[2])));
const crossencoder = new CrossEncoder(model, 4096);

const query = 'What is the capital of France?';
const documents = [
  'Paris is the capital and largest city of France.',
  'The Eiffel Tower is located in Paris, France.',
  'Berlin is the capital of Germany.',
  'London is the capital of the United Kingdom.',
  'France is a country in Western Europe.',
  'The French Revolution began in 1789.',
  'Tokyo is the capital of Japan.',
  'French cuisine is famous worldwide.',
];

const ranked = await crossencoder.rankAndSort(query, documents);
for (const [doc, score] of ranked) {
  console.log(`${score.toFixed(3)} - ${doc}`);
}
