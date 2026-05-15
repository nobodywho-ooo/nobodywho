// Simple chat demo. Mirrors python/examples/small_model_demo.py.
// Usage: node js/examples/chat_demo.mjs /path/to/model.gguf

import { readFileSync } from 'node:fs';
import { Model, Chat } from './setup.mjs';

const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: node chat_demo.mjs /path/to/model.gguf');
  process.exit(1);
}

const model = await Model.loadBytes(new Uint8Array(readFileSync(modelPath)));
const chat = new Chat(model, { systemPrompt: 'You are a helpful assistant' });

const result = await (await chat.ask('What is the capital of Denmark?')).completed();
console.log(result);

if (!result.toLowerCase().includes('copenhagen')) {
  throw new Error('Model does not know the capital of Denmark.');
}
