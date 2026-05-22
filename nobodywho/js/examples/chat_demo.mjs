import { readFileSync } from 'node:fs';
import { strict as assert } from 'node:assert';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });
m.init();

const chat = await m.Chat.create({
  modelBytes: new Uint8Array(readFileSync(process.argv[2])),
  systemPrompt: 'You are a helpful assistant',
  templateVariables: { enable_thinking: false },
});

const result = await chat.ask('What is the capital of Denmark?').completed();
console.log(result);
assert(result.toLowerCase().includes('copenhagen'), 'Model does not know the capital of Denmark.');

await chat.terminate();
process.exit(0);
