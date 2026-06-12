---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_position: 4
---

The model does not produce tokens but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a **sampler**, which using NobodyWho you can freely modify, to achieve better quality outputs or constrain the outputs to some known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers a couple of well-known presets, which you can quickly utilize.
For example, if you want to increase or decrease the "creativity" of your model, select our `temperature` preset:

```swift
import NobodyWho

let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    sampler: SamplerPresets.temperature(0.2)
)
```

Setting `temperature` to `0.2` will affect the sampler when choosing the next token, making the distribution less flat and therefore the model will favour more probable tokens.

To see the whole list of presets, check out the `SamplerPresets` enum:

```swift
enum SamplerPresets {
    static func `default`() -> SamplerConfig
    static func dry() -> SamplerConfig
    static func greedy() -> SamplerConfig
    static func temperature(_ temperature: Float) -> SamplerConfig
    static func topK(_ topK: Int32) -> SamplerConfig
    static func topP(_ topP: Float) -> SamplerConfig

    // Constrain output to a specific format:
    static func constrainWithJsonSchema(_ schema: String) -> SamplerConfig
    static func constrainWithRegex(_ pattern: String) -> SamplerConfig
    static func constrainWithGrammar(_ grammar: String) -> SamplerConfig
}
```

## Structured output

One of the most useful features is constraining the model to produce structured output —
this gives you a hard guarantee that the output matches a specific format, rather than
relying on the model to get it right on its own.

### Regular expressions

For simpler patterns, you can constrain the output with a regex:
```swift
// Force the model to answer with exactly "yes" or "no"
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    sampler: SamplerPresets.constrainWithRegex("yes|no")
)
let answer = try await chat.ask("Is the sky blue?").completed()
```

### JSON schema

In some use-cases it might be useful to let the LLM generate JSON output.
You can use a JSON schema to force the LLM to produce the exact object shape you need:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    sampler: SamplerPresets.constrainWithJsonSchema("""
    {
      "type": "object",
      "properties": {
        "name": { "type": "string", "maxLength": 50 },
        "age":  { "type": "integer" }
      },
      "required": ["name", "age"],
      "additionalProperties": false
    }
    """)
)
let response = try await chat.ask("Give me a person as JSON with name and age fields.").completed()
// `response` is always valid JSON matching the schema
```

### Custom grammars (advanced)

For cases where JSON schema and regex are not expressive enough, you can supply a custom grammar.
`constrainWithGrammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format) —
NobodyWho automatically converts GBNF to Lark before passing it to the inference engine.

**Lark syntax** (recommended):
```swift
let sampler = SamplerPresets.constrainWithGrammar("""
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,"\\n\\r]+/
    NEWLINE: /\\r?\\n/
""")
```

**GBNF syntax** (also accepted):
```swift
let sampler = SamplerPresets.constrainWithGrammar("""
    file   ::= record (newline record)* newline?
    record ::= field ("," field)*
    field  ::= /[^,"\\n\\r]+/
    newline ::= "\\r\\n" | "\\n"
""")
```

See the [Lark documentation](https://lark-parser.readthedocs.io/en/latest/grammar.html) and the
[GBNF specification](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md) for the
full grammar syntax.

:::info
The older `SamplerPresets.json()` and `SamplerPresets.grammar()` methods are deprecated.
Use `SamplerPresets.constrainWithJsonSchema()` for JSON output or
`SamplerPresets.constrainWithGrammar()` for custom grammars — the latter accepts both Lark and GBNF strings.
:::

## Defining your own samplers

Sampler presets abstract away some control that you might want - for example, if you
want to chain samplers, change more "advanced" parameters, etc. For that use case,
we provide the `SamplerBuilder` class:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    sampler: SamplerBuilder().temperature(0.8).topK(5).dist()
)
```

With `SamplerBuilder` you can chain multiple steps together and then select how you
want to sample from the distribution. Keep in mind that `SamplerBuilder` provides two
types of methods: ones which modify the distribution (returning again the instance of
`SamplerBuilder`) and ones which sample from the distribution (returning `SamplerConfig`).
So in order to have the sampler working properly, be careful
to always end the chain with one of the sampling steps (e.g. `dist()`, `greedy()`, `mirostatV2()`, etc.).

For reproducible output, set the RNG seed with `.seed(seed:)` anywhere in the chain.
It is consumed by every random sampler in the chain — `dist`, `mirostatV1`, `mirostatV2`,
and the `xtc` shift step. `greedy` ignores it. If unset, a default seed is used.

```swift
let sampler = SamplerBuilder().temperature(0.8).topK(5).seed(seed: 42).dist()
```

You can also change the sampler configuration on an existing chat instance:

```swift
let sampler = SamplerBuilder().temperature(0.8).topK(5).dist()
try await chat.setSamplerConfig(sampler)
```
