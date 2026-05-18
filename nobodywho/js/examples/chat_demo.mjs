import { readFileSync } from 'node:fs';
import { strict as assert } from 'node:assert';
import { Model, Chat } from './setup.mjs';

const model = await Model.loadBytes(new Uint8Array(readFileSync(process.argv[2])));
const chat = new Chat(model, {
  systemPrompt: 'You are a helpful assistant',
  templateVariables: { enable_thinking: false },
});

const stream = await chat.ask('What is the capital of Denmark?');
const result = await stream.completed();
console.log(result);
assert(result.toLowerCase().includes('copenhagen'), 'Model does not know the capital of Denmark.');
