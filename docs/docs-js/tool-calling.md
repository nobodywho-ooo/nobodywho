---
title: Tool Calling
description: Letting the model call your JavaScript functions
sidebar_position: 2
---

Tools let the model call your own JavaScript functions mid-conversation — to fetch data, do math, hit an API, and so on. The model decides *when* to call a tool from its name, description, and argument schema; NobodyWho runs your callback and feeds the result back so generation continues.

## Defining a tool

Create a tool with `Tool.fromFn(name, description, schema, callback)`:

```js
import createNobodyWhoModule from '@nobodywho/js';
const m = await createNobodyWhoModule();

const getWeather = m.Tool.fromFn(
  'get_weather',
  'Get the current weather for a city.',
  {
    type: 'object',
    properties: {
      city: { type: 'string', description: 'City name in English' },
    },
    required: ['city'],
  },
  (args) => `Sunny in ${args.city}, 21°C.`,
);
```

- **name** / **description** — how the model decides whether to call the tool. Be descriptive.
- **schema** — a JSON Schema for the arguments. The model is constrained to produce arguments matching it.
- **callback** — receives the parsed arguments object and returns a string (the tool's result).

## Using tools in a chat

Pass tools when creating the chat:

```js
const chat = await m.Chat.create({
  modelUrl: '…',
  systemPrompt: 'When the user asks about weather, use the get_weather tool.',
  tools: [getWeather],
});

const reply = await chat.ask('What is the weather in Copenhagen?').completed();
// Behind the scenes the model calls get_weather({ city: 'Copenhagen' }),
// then answers using the result.
```

You can also swap the tool set on an existing chat with `chat.setTools([...])`.

## Async tools

Callbacks may be `async` (return a Promise) — ideal for network requests or anything I/O-bound. Inference pauses at the tool boundary and resumes once your Promise resolves:

```js
const lookupOrder = m.Tool.fromFn(
  'lookup_order',
  'Look up an order by its id.',
  {
    type: 'object',
    properties: { id: { type: 'string' } },
    required: ['id'],
  },
  async (args) => {
    const res = await fetch(`/api/orders/${args.id}`);
    return await res.text();
  },
);
```

:::tip
Smaller models are less reliable at following argument schemas. If tool arguments matter, prefer a larger model and keep schemas simple.
:::
