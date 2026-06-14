---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_position: 4
---

The model does not produce tokens but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a **sampler**, which using NobodyWho you can freely modify, to achieve better quality outputs or constrain the outputs to some known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers a couple of well-known presets, which you can quickly utilize.
For example, if you want to increase or decrease the "creativity" of your model, select our `temperature` preset:

```typescript
import { Chat, SamplerPresets } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  sampler: SamplerPresets.temperature(0.2),
});
```

Setting `temperature` to `0.2` will affect the sampler when choosing the next token, making the distribution less flat and therefore the model will favour more probable tokens.

To see the whole list of presets, check out the `SamplerPresets` class:

```typescript
class SamplerPresets {
  static default(): SamplerConfig;
  static dry(): SamplerConfig;
  static greedy(): SamplerConfig;
  static json(): SamplerConfig;
  static temperature(temperature: number): SamplerConfig;
  static topK(topK: number): SamplerConfig;
  static topP(topP: number): SamplerConfig;

  // Constrain output to a specific format:
  static constrainWithJsonSchema(schema: string): SamplerConfig;
  static constrainWithRegex(pattern: string): SamplerConfig;
  static constrainWithGrammar(grammar: string): SamplerConfig;
}
```

## Structured output

One of the most useful features is constraining the model to produce structured output —
this gives you a hard guarantee that the output matches a specific format, rather than
relying on the model to get it right on its own.

### Regular expressions

For simpler patterns, you can constrain the output with a regex. Both regex literals and strings are accepted:

```typescript
// Force the model to answer with exactly "yes" or "no"
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  sampler: SamplerPresets.constrainWithRegex(/yes|no/),
});
const answer = await chat.ask("Is the sky blue?").completed();
```

### JSON schema

In some use-cases it might be useful to let the LLM generate JSON output.
This could be done either in the simple way, just enforcing any JSON by the preset:

```typescript
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  sampler: SamplerPresets.json(),
});
```

Or utilizing JSON schemas to really force the LLM to give you the specific object shapes
that you want:

```typescript
const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  sampler: SamplerPresets.constrainWithJsonSchema({
    type: "object",
    properties: {
      name: { type: "string", maxLength: 50 },
      age:  { type: "integer" },
    },
    required: ["name", "age"],
    additionalProperties: false,
  }),
});
const response = await chat.ask("Give me a person as JSON with name and age fields.").completed();
const person = JSON.parse(response); // always valid JSON matching the schema
```

### Custom grammars (advanced)

For cases where JSON schema and regex are not expressive enough, you can supply a custom grammar.
`constrainWithGrammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format) —
NobodyWho automatically converts GBNF to Lark before passing it to the inference engine.

**Lark syntax** (recommended):

```typescript
const sampler = SamplerPresets.constrainWithGrammar(`
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,"\\n\\r]+/
    NEWLINE: /\\r?\\n/
`);
```

**GBNF syntax** (also accepted):

```typescript
const sampler = SamplerPresets.constrainWithGrammar(`
    file   ::= record (newline record)* newline?
    record ::= field ("," field)*
    field  ::= /[^,"\\n\\r]+/
    newline ::= "\\r\\n" | "\\n"
`);
```

See the [Lark documentation](https://lark-parser.readthedocs.io/en/latest/grammar.html) and the
[GBNF specification](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md) for the
full grammar syntax.

:::info
The older `SamplerPresets.grammar()` method is deprecated. Use
`SamplerPresets.constrainWithGrammar()` instead — it accepts both Lark and GBNF strings.
:::

## Defining your own samplers

Sampler presets abstract away some control that you might want - for example, if you
want to chain samplers, change more "advanced" parameters, etc. For that use case,
we provide the `SamplerBuilder` class:

```typescript
import { Chat, SamplerBuilder, SamplerConfig } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  sampler: new SamplerBuilder().temperature(0.8).topK(5).dist() as SamplerConfig,
});
```

With `SamplerBuilder` you can chain multiple steps together and then select how you
want to sample from the distribution. Keep in mind that `SamplerBuilder` provides two
types of methods: ones which modify the distribution (returning again the instance of
`SamplerBuilder`) and ones which sample from the distribution (returning `SamplerConfig`).
So in order to have the sampler working properly, be careful
to always end the chain with one of the sampling steps (e.g. `dist()`, `greedy()`, `mirostatV2()`, etc.).

For reproducible output, set the RNG seed with `.seed(value)` anywhere in the chain.
It is consumed by every random sampler in the chain — `dist`, `mirostatV1`, `mirostatV2`,
and the `xtc` shift step. `greedy` ignores it. If unset, a default seed is used.

```typescript
const sampler = new SamplerBuilder().temperature(0.8).topK(5).seed(42).dist() as SamplerConfig;
```

### Available sampling steps

Pick any of the **shift steps** below (each reshapes the token distribution), then finish with one **terminal step** that picks the token — exactly like the `.temperature(0.8).topK(5).dist()` chain above.

Shift steps — add as many as you want, applied in order:

- `.topK(40)` — keep only the 40 most likely tokens
- `.topP(0.95, 1)` — nucleus: keep the top tokens up to 95% of the probability mass
- `.minP(0.05, 1)` — drop tokens below 5% of the most likely token's probability
- `.typicalP(0.9, 1)` — keep tokens with "typical" information content
- `.xtc(0.5, 0.1, 1)` — occasionally drop the top tokens for more variety
- `.temperature(0.8)` — below 1.0 = more focused, above 1.0 = more random
- `.penalties(64, 1.1, 0.0, 0.0)` — per-token repetition penalty: `penaltyLastN, penaltyRepeat, penaltyFreq, penaltyPresent` (`penaltyRepeat` 1.0 = off)
- `.dry(0.8, 1.75, 2, -1, ["\n"])` — penalty for repeated *phrases*: `multiplier, base, allowedLength, penaltyLastN, seqBreakers`
- `.seed(42)` — fix the RNG for reproducible output
- `.grammar(...)` — deprecated; use the `constrainWith*` presets above

Terminal step — end the chain with exactly one:

- `.dist()` — pick a token with weighted randomness (the usual choice)
- `.greedy()` — always take the most likely token
- `.mirostatV1(5.0, 0.1, 100)` / `.mirostatV2(5.0, 0.1)` — steer output "surprise" toward a target

`minKeep` is the floor on how many tokens survive a cut (`1` is fine). Terminal steps return a `SamplerConfig`, so cast with `as SamplerConfig` as in the examples.

You can also change the sampler configuration on an existing chat instance:

```typescript
const sampler = new SamplerBuilder()
  .temperature(0.8)
  .topK(5)
  .dist() as SamplerConfig;

await chat.setSamplerConfig(sampler);
```
