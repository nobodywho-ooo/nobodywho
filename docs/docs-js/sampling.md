---
title: Sampling
description: Controlling how the model picks tokens
sidebar_position: 4
---

Sampling controls how the model chooses each next token — from fully deterministic (greedy) to creative (higher temperature). NobodyWho gives you presets, a fluent builder, and regex / JSON-schema / grammar constraints.

## Presets

`SamplerPresets` returns ready-made configs:

```js
const m = await createNobodyWhoModule();

m.SamplerPresets.default();         // sensible general-purpose defaults
m.SamplerPresets.greedy();          // deterministic — always the top token
m.SamplerPresets.temperature(0.8);  // temperature only
m.SamplerPresets.topK(40);
m.SamplerPresets.topP(0.95);
```

Pass one to `Chat.create` via the `sampler` option:

```js
const chat = await m.Chat.create({
  modelUrl: '…',
  sampler: m.SamplerPresets.temperature(0.7),
});
```

## Builder

For finer control, chain options with `SamplerBuilder` and finish with a terminal step — `.dist()` to sample, or `.greedy()` for deterministic output:

```js
const sampler = new m.SamplerBuilder()
  .topK(40)
  .topP(0.95)
  .temperature(0.7)
  .dist();

const chat = await m.Chat.create({ modelUrl: '…', sampler });
```

Read or replace the sampler on an existing chat with `getSamplerConfig()` / `setSamplerConfig(sampler)`.

## Constrained / structured output

You can force the output to match a regular expression, a JSON Schema, or a grammar — handy for machine-readable responses:

```js
// Only digits:
const sampler = m.SamplerPresets.constrainWithRegex('^[0-9]{1,3}$');

// Conform to a JSON Schema:
const jsonSampler = m.SamplerPresets.constrainWithJsonSchema({
  type: 'object',
  properties: { city: { type: 'string' }, population: { type: 'number' } },
  required: ['city', 'population'],
});

const chat = await m.Chat.create({ modelUrl: '…', sampler: jsonSampler });
const json = await chat.ask('Give me a city and its population.').completed();
JSON.parse(json); // guaranteed to parse
```

A grammar constraint is available too, via `m.SamplerPresets.constrainWithGrammar(...)`.

:::tip
Constraints guarantee the *shape* of the output, not its correctness — the model still has to fill in sensible values.
:::
