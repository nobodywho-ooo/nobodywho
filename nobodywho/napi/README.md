# nobodywho

Run LLMs locally with offline inference in Node.js and Electron. No API keys, no cloud — everything runs on your machine.

Built on [llama.cpp](https://github.com/ggerganov/llama.cpp) with GPU acceleration via Vulkan (Linux/Windows) and Metal (macOS).

## Install

```bash
npm install nobodywho
```

The correct native binary for your platform is installed automatically.

## Quick start

```typescript
import { Model, Chat, streamTokens } from 'nobodywho';

// Load a GGUF model (with GPU acceleration)
const model = await Model.load('path/to/model.gguf', true);

// Create a chat session
const chat = new Chat(model, 'You are a helpful assistant.');

// Stream the response token by token
for await (const token of streamTokens(chat.ask('What is the meaning of life?'))) {
  process.stdout.write(token);
}
```

## API

### Model

```typescript
// Load a model from a GGUF file
const model = await Model.load(modelPath: string, useGpu: boolean, imageModelPath?: string);

// Check if a discrete GPU is available
const hasGpu = Model.hasDiscreteGpu();
```

### Chat

```typescript
// Create a chat session
const chat = new Chat(model, systemPrompt?, contextSize?, templateVariables?, tools?, sampler?);

// Send a message and stream the response
const stream = chat.ask('Hello!');

// Option 1: Stream tokens
for await (const token of streamTokens(stream)) {
  process.stdout.write(token);
}

// Option 2: Get the full response
const response = await stream.completed();

// Stop generation early
chat.stopGeneration();

// Manage conversation
await chat.resetHistory();
await chat.resetContext(newSystemPrompt?, newTools?);
const history = await chat.getChatHistory();
await chat.setChatHistory(messages);
```

### Tool calling

```typescript
const weatherTool = new Tool(
  'get_weather',
  'Get the current weather for a city',
  JSON.stringify({
    type: 'object',
    properties: { city: { type: 'string' } },
    required: ['city'],
  }),
  (argsJson) => {
    const { city } = JSON.parse(argsJson);
    return JSON.stringify({ temp: 22, condition: 'sunny' });
  },
);

const chat = new Chat(model, 'You are helpful.', 4096, null, [weatherTool]);

for await (const token of streamTokens(chat.ask('What is the weather in London?'))) {
  process.stdout.write(token);
}
```

### Embeddings

```typescript
import { Encoder, cosineSimilarity } from 'nobodywho';

const encoder = new Encoder(model);
const a = await encoder.encode('cats are great');
const b = await encoder.encode('dogs are wonderful');
const similarity = cosineSimilarity(a, b);
```

### Cross-encoder (reranking)

```typescript
import { CrossEncoder } from 'nobodywho';

const ranker = new CrossEncoder(model);
const scores = await ranker.rank('best pet', ['cats are great', 'taxes are due']);
const sorted = await ranker.rankAndSort('best pet', ['cats are great', 'taxes are due']);
```

### Sampler configuration

```typescript
import { SamplerPresets, SamplerBuilder } from 'nobodywho';

// Use a preset
const chat = new Chat(model, 'You are helpful.', 4096, null, null, SamplerPresets.temperature(0.7));

// Or build a custom sampler chain
const sampler = new SamplerBuilder()
  .topK(40)
  .topP(0.95, 1)
  .temperature(0.8)
  .dist();

const chat2 = new Chat(model, 'You are helpful.', 4096, null, null, sampler);
```

## Supported platforms

| Platform | Architecture | GPU |
|----------|-------------|-----|
| Linux | x86_64, aarch64 | Vulkan |
| macOS | x86_64, Apple Silicon | Metal |
| Windows | x86_64 | Vulkan |

## Models

This library works with any GGUF model file. You can find models on [Hugging Face](https://huggingface.co/models?library=gguf).

## Learn more

- [Documentation](https://docs.nobodywho.ooo)
- [GitHub](https://github.com/nobodywho-ooo/nobodywho)
- [Contributing](https://github.com/nobodywho-ooo/nobodywho/blob/main/CONTRIBUTING.md)
