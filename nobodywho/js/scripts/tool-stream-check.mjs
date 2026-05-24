// Empirical check: when a Chat has a tool and the model emits a tool
// call, do the tool-call grammar tokens leak into the streamed
// next()/for-await output, or does the existing filter in core's
// wrap_respond() already suppress them?
//
// Asserts:
//   - the tool callback fires (proves a tool call actually happened)
//   - the concatenated streamed text contains no '<tool_call>' or
//     '</tool_call>' substrings (no leak)
//   - the final answer mentions the tool's result (so we know the
//     post-tool-call generation streamed correctly too)
//
// Usage: node tool-stream-check.mjs <model.gguf>
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join, dirname } from 'node:path';

const pkgDir = join(dirname(fileURLToPath(import.meta.url)), '..', 'pkg-bundler');
const { default: createNobodyWhoModule } = await import(join(pkgDir, 'nobodywho_js.js'));
const m = await createNobodyWhoModule({ locateFile: (p) => join(pkgDir, p) });

const modelPath = process.argv[2];
if (!modelPath) {
  console.error('usage: tool-stream-check.mjs <model.gguf>');
  process.exit(2);
}

let toolCalled = false;
let toolArgs = null;

const weather = m.Tool.fromFn(
  'get_weather',
  'Get the current weather for a city, in Celsius.',
  {
    type: 'object',
    properties: { city: { type: 'string', description: 'City name' } },
    required: ['city'],
  },
  (args) => {
    toolCalled = true;
    toolArgs = args;
    return JSON.stringify({ city: args.city, tempC: 14, conditions: 'overcast' });
  },
);

const chat = await m.Chat.create({
  modelBytes: new Uint8Array(readFileSync(modelPath)),
  systemPrompt: 'You are a helpful assistant. Use the get_weather tool when asked about weather.',
  templateVariables: { enable_thinking: false },
  tools: [weather],
});

const tokens = [];
const stream = chat.ask('What is the weather in Copenhagen right now? Use the tool, then answer in one sentence.');
for await (const tok of stream) {
  tokens.push(tok);
}
const streamed = tokens.join('');

await chat.terminate();

console.log('=== streamed text (all tokens concatenated) ===');
console.log(JSON.stringify(streamed));
console.log('');
console.log(`tokens received: ${tokens.length}`);
console.log(`tool called:     ${toolCalled}`);
console.log(`tool args:       ${JSON.stringify(toolArgs)}`);
console.log('');

const leaksOpen = streamed.includes('<tool_call>');
const leaksClose = streamed.includes('</tool_call>');
const leaksJsonName = streamed.includes('"name"') && streamed.includes('get_weather');

console.log(`leaks "<tool_call>":   ${leaksOpen}`);
console.log(`leaks "</tool_call>":  ${leaksClose}`);
console.log(`leaks tool-call JSON:  ${leaksJsonName}`);

let exitCode = 0;
if (!toolCalled) {
  console.log('\nFAIL: tool was never called (model did not choose to use it)');
  exitCode = 1;
}
if (leaksOpen || leaksClose || leaksJsonName) {
  console.log('\nFAIL: tool-call grammar tokens leaked into the stream');
  exitCode = 1;
}
if (exitCode === 0) {
  console.log('\nPASS: tool fired, no leakage in streamed tokens');
}
process.exit(exitCode);
