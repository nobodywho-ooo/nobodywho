import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const model = await m.Model.load({ modelPath: process.argv[2] });
const crossencoder = new m.CrossEncoder(model, 4096);

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
