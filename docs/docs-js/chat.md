---
title: Chat
description: Creating and configuring a Chat in JavaScript
sidebar_position: 1
---

As you saw in the [welcome guide](./), every interaction starts by creating a `Chat`. This page covers its options and the methods on it.

## Creating a Chat

`Chat.create` is an async factory — it loads the model and spins up a background worker for inference:

```js
import createNobodyWhoModule from '@nobodywho/js';
const m = await createNobodyWhoModule();

// Browser — fetch + cache the model from a URL:
const chat = await m.Chat.create({
  modelUrl: 'https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
});

// Node — load from a local path:
const chat = await m.Chat.create({ modelPath: '/path/to/model.gguf' });
```

In the browser the model is fetched once and cached in the Cache API, so later loads are instant. In Node, `modelPath` reads straight from disk.

## Prompts and responses

`chat.ask()` sends a message and starts generating. It returns a `TokenStream`:

```js
const stream = chat.ask('Is water wet?');
```

To render tokens as they arrive, async-iterate the stream — each token is a word or word fragment:

```js
for await (const token of chat.ask('Write a short paragraph about Copenhagen.')) {
  process.stdout.write(token);     // Node
  // outputEl.textContent += token; // browser
}
```

If you only want the final text, await `completed()`:

```js
const reply = await chat.ask('Is water wet?').completed();
```

Messages and responses are stored on the `Chat`, so the next `ask()` remembers the conversation.

## Stopping generation

To cancel an in-flight response (for example, a "Stop" button), call `stopGeneration()`. Tokens already produced stay in the stream, and the partial reply is kept in history so the conversation stays coherent:

```js
chat.stopGeneration();
```

## System prompt

A system prompt guides overall behavior. Set it when creating the chat:

```js
const chat = await m.Chat.create({
  modelUrl: '…',
  systemPrompt: 'You are a mischievous assistant!',
});
```

…or change it later with `setSystemPrompt(prompt)` (pass `null` to clear it and fall back to the model's default), and read it back with `getSystemPrompt()`.

## Context size

The context is how many tokens the model keeps in memory for the conversation. Larger contexts cost more memory and compute. Set it at creation:

```js
const chat = await m.Chat.create({ modelUrl: '…', contextSize: 4096 });
```

When a conversation fills the context, NobodyWho automatically shrinks it (dropping the oldest messages while keeping the system prompt) and updates the KV cache for you. This matters most in the browser, where wasm32 caps total memory at 4 GiB.

## Chat history

Read the stored messages with `getChatHistory()`, and replace them with `setChatHistory()` — it round-trips the same shape:

```js
const history = await chat.getChatHistory();
// each entry has a `role` ('system' | 'user' | 'assistant') and `content`

await chat.setChatHistory(history);
```

## Resetting

- `chat.resetHistory()` — clear the conversation but keep the system prompt, tools, and sampler.
- `chat.reset({ systemPrompt, tools })` — clear history and atomically swap the system prompt and/or tools.

## Releasing resources

`chat.terminate()` stops any in-flight generation and shuts down the worker, freeing the model. Call it when you're done with a chat — especially in the browser, to release memory:

```js
await chat.terminate();
```

## Template variables

Chat templates format the conversation into the model's expected prompt shape. Some models expose boolean template variables — for example, Qwen3 supports `enable_thinking`:

```js
const chat = await m.Chat.create({
  modelUrl: '…',
  templateVariables: { enable_thinking: false },
});
```

You can also change them on an existing chat with `setTemplateVariable(name, value)` / `setTemplateVariables(vars)`, and read them with `getTemplateVariables()`.

:::info
Template variables are model-specific — if a model's template doesn't use one, it's ignored gracefully.
:::
