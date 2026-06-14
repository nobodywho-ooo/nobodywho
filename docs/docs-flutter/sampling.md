---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_position: 4
---

The model does not produce tokens but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a **sampler**, which using NobodyWho you can freely modify,
to achieve better quality outputs or constrain the outputs to some known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers a couple of well-known presets, which you can quickly utilize.
For example, if you want to increase or decrease the "creativity" of your model, select our `temperature` preset:
```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  sampler: nobodywho.SamplerPresets.temperature(temperature: 0.2)
);
```
Setting `temperature` to `0.2`, will then affect the sampler when choosing the next token, making the distribution less flat and therefore the model will favour more probable tokens.

To see the whole list of presets, check out the `SamplerPresets` class:
```dart
class SamplerPresets {
  static SamplerConfig defaultSampler();
  static SamplerConfig dry();
  static SamplerConfig greedy();
  static SamplerConfig json();
  static SamplerConfig temperature({required double temperature});
  static SamplerConfig topK({required int topK});
  static SamplerConfig topP({required double topP});

  // Constrain output to a specific format:
  static SamplerConfig constrainWithJsonSchema({required String schema});
  static SamplerConfig constrainWithRegex({required String pattern});
  static SamplerConfig constrainWithGrammar({required String grammar});
}
```

## Structured output

One of the most useful features is constraining the model to produce structured output —
this gives you a hard guarantee that the output matches a specific format, rather than
relying on the model to get it right on its own.

### Regular expressions

For simpler patterns, you can constrain the output with a regex:
```dart
// Force the model to answer with exactly "yes" or "no"
final chat = await nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  sampler: nobodywho.SamplerPresets.constrainWithRegex(pattern: r'yes|no'),
);
final answer = await chat.ask("Is the sky blue?").completed();
```

### JSON schema

In some use-cases it might be useful to let the LLM generate JSON output.
This could be done either in the simple way, just enforcing any JSON by the preset:
```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  sampler: nobodywho.SamplerPresets.json(),
);
```

Or utilizing JSON schemas to really force the LLM to give you the specific object shapes
that you want:
```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  sampler: nobodywho.SamplerPresets.constrainWithJsonSchema(schema: {
    'type': 'object',
    'properties': {
      'name': {'type': 'string', 'maxLength': 50},
      'age':  {'type': 'integer'},
    },
    'required': ['name', 'age'],
    'additionalProperties': false,
  }),
);
final response = await chat.ask("Give me a person as JSON with name and age fields.").completed();
final person = jsonDecode(response); // always valid JSON matching the schema
```

### Custom grammars (advanced)

For cases where JSON schema and regex are not expressive enough, you can supply a custom grammar.
`constrainWithGrammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format) —
NobodyWho automatically converts GBNF to Lark before passing it to the inference engine.

**Lark syntax** (recommended):
```dart
final sampler = nobodywho.SamplerPresets.constrainWithGrammar(grammar: """
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,"\\n\\r]+/
    NEWLINE: /\\r?\\n/
""");
```

**GBNF syntax** (also accepted):
```dart
final sampler = nobodywho.SamplerPresets.constrainWithGrammar(grammar: """
    file   ::= record (newline record)* newline?
    record ::= field ("," field)*
    field  ::= /[^,"\\n\\r]+/
    newline ::= "\\r\\n" | "\\n"
""");
```

See the [Lark documentation](https://lark-parser.readthedocs.io/en/latest/grammar.html) and the
[GBNF specification](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md) for the
full grammar syntax.

:::info
The older `SamplerPresets.grammar()` method is deprecated. Use
`SamplerPresets.constrainWithGrammar()` instead — it accepts both Lark and GBNF strings.
:::


## Defining your own samplers

Sampler presets abstract away some control, that you might want - for example, if you
want to chain samplers, change more "advanced" parameters, etc. For that use case,
we provide `SamplerBuilder` class:
```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final chat = await nobodywho.Chat.fromPath(
  modelPath: "./model.gguf",
  sampler: nobodywho.SamplerBuilder()
      .temperature(temperature: 0.8)
      .topK(topK: 5)
      .dist()
);
```
With `SamplerBuilder` you can chain multiple steps together and then select how do you
want to sample from the distribution. Keep in mind, that `SamplerBuilder` provides two
types of methods: ones which modify the distribution (returning again the instance of
`SamplerBuilder`) and ones which sample from the distribution (returning `SamplerConfig`).
So in order to have the sampler working properly and not giving you type errors, be careful
to always end the chain with one of the sampling steps (e.g. `dist()`, `greedy()`, `mirostatV2()`, etc.).

For reproducible output, set the RNG seed with `.seed(seed: value)` anywhere in the chain.
It is consumed by every random sampler in the chain — `dist`, `mirostatV1`, `mirostatV2`,
and the `xtc` shift step. `greedy` ignores it. If unset, a default seed is used.

```dart
final sampler = nobodywho.SamplerBuilder()
    .temperature(temperature: 0.8)
    .topK(topK: 5)
    .seed(seed: 42)
    .dist();
```

### Available sampling steps

The full set of steps you can chain on a `SamplerBuilder`:

```dart
class SamplerBuilder {
  // Shift steps — chain any number; applied in the order you add them:
  SamplerBuilder topK({required int topK});
  SamplerBuilder topP({required double topP, required int minKeep});
  SamplerBuilder minP({required double minP, required int minKeep});
  SamplerBuilder typicalP({required double typP, required int minKeep});
  SamplerBuilder xtc({required double xtcProbability, required double xtcThreshold, required int minKeep});
  SamplerBuilder temperature({required double temperature});
  SamplerBuilder penalties({required int penaltyLastN, required double penaltyRepeat, required double penaltyFreq, required double penaltyPresent}); // repetition penalty, per token
  SamplerBuilder dry({required double multiplier, required double base, required int allowedLength, required int penaltyLastN, required List<String> seqBreakers}); // repetition penalty, for repeated phrases/sequences
  SamplerBuilder seed({required int seed});
  SamplerBuilder grammar({required String grammar, String? triggerOn, required String root}); // deprecated: use the constrainWith* presets

  // Sampling steps — end the chain with exactly one:
  SamplerConfig dist();
  SamplerConfig greedy();
  SamplerConfig mirostatV1({required double tau, required double eta, required int m});
  SamplerConfig mirostatV2({required double tau, required double eta});
}
```

Steps that take `minKeep` always keep at least that many candidate tokens, regardless of the cutoff (`1` is a sensible default).

You can also change the sampler configuration on an existing chat instance:

```dart
final chat = await nobodywho.Chat.fromPath(modelPath: "./model.gguf");

final sampler = nobodywho.SamplerBuilder()
    .temperature(temperature: 0.8)
    .topK(topK: 5)
    .dist();

await chat.setSamplerConfig(sampler);
```

