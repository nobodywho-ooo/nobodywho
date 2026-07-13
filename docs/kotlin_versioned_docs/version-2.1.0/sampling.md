---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_position: 4
---

The model does not produce tokens but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a **sampler**, which you can freely configure to achieve better quality outputs or constrain outputs to a known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers well-known presets. For example, to adjust the "creativity" of your model:

```kotlin
import ai.nobodywho.Chat
import ai.nobodywho.SamplerPresets

val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    sampler = SamplerPresets.temperature(0.2f)
)
```

Setting `temperature` to `0.2` makes the distribution less flat, so the model favours more probable tokens.

The full list of presets:

```kotlin
object SamplerPresets {
    fun default(): SamplerConfig
    fun dry(): SamplerConfig
    fun greedy(): SamplerConfig
    fun json(): SamplerConfig
    fun temperature(temperature: Float): SamplerConfig
    fun topK(topK: Int): SamplerConfig
    fun topP(topP: Float): SamplerConfig

    // Constrain output to a specific format:
    fun constrainWithJsonSchema(schema: String): SamplerConfig
    fun constrainWithRegex(pattern: String): SamplerConfig
    fun constrainWithGrammar(grammar: String): SamplerConfig
}
```

## Structured output

One of the most useful features is constraining the model to produce structured output — this gives you a hard guarantee that the output matches a specific format.

### Regular expressions

For simpler patterns, constrain the output with a regex:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    sampler = SamplerPresets.constrainWithRegex("yes|no")
)
val answer = chat.ask("Is the sky blue?").completed()
// answer is guaranteed to be exactly "yes" or "no"
```

### JSON schema

Enforce any JSON output:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    sampler = SamplerPresets.json()
)
```

Or use a JSON schema for specific object shapes:

```kotlin
val schema = """
{
    "type": "object",
    "properties": {
        "name": {"type": "string", "maxLength": 50},
        "age": {"type": "integer"}
    },
    "required": ["name", "age"],
    "additionalProperties": false
}
"""

val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    sampler = SamplerPresets.constrainWithJsonSchema(schema)
)
val response = chat.ask("Give me a person with name and age.").completed()
// response is always valid JSON matching the schema
```

### Custom grammars (advanced)

For cases where JSON schema and regex are not expressive enough, supply a custom grammar. `constrainWithGrammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format).

**Lark syntax** (recommended):

```kotlin
val sampler = SamplerPresets.constrainWithGrammar("""
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,"\n\r]+/
    NEWLINE: /\r?\n/
""")
```

**GBNF syntax** (also accepted):

```kotlin
val sampler = SamplerPresets.constrainWithGrammar("""
    file   ::= record (newline record)* newline?
    record ::= field ("," field)*
    field  ::= /[^,"\n\r]+/
    newline ::= "\r\n" | "\n"
""")
```

:::info
The older `SamplerPresets.grammar()` method is deprecated. Use
`SamplerPresets.constrainWithGrammar()` instead — it accepts both Lark and GBNF strings.

:::

## Building custom samplers with the DSL

Sampler presets abstract away some control. For more advanced configurations — chaining samplers, tuning parameters — use the `buildSampler` DSL:

```kotlin
import ai.nobodywho.buildSampler

val sampler = buildSampler {
    topK(40)
    temperature(0.8)
    minP(0.05)
    dist()
}

val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    sampler = sampler
)
```

### Available sampling steps

Inside `buildSampler { }`, call any of the **shift steps** below (each reshapes the distribution), then one **terminal step** that picks the token. Most steps have defaults, so you only pass what you want to change.

Shift steps — call as many as you want, in order:

- `topK(40)` — keep only the 40 most likely tokens
- `topP(0.95)` — nucleus: keep the top tokens up to 95% of the probability mass
- `minP(0.05)` — drop tokens below 5% of the most likely token's probability
- `typicalP(0.9)` — keep tokens whose "surprise" is close to average, dropping both the too-predictable and the too-random ([locally typical sampling](https://arxiv.org/abs/2202.00666))
- `xtc(0.5, 0.1)` — "exclude top choices": occasionally drop the top tokens for more variety
- `temperature(0.8)` — below 1.0 = more focused, above 1.0 = more random
- `penalties(penaltyRepeat = 1.1)` — per-token repetition penalty (`penaltyRepeat` 1.0 = off)
- `dry()` — penalty for repeated *phrases* (its defaults are a good start)
- `seed(42)` — fix the RNG for reproducible output
- `grammar(...)` — deprecated; use the `constrainWith*` presets above

Terminal step — call at most one:

- `dist()` — pick a token with weighted randomness (used by default if you omit it)
- `greedy()` — always take the most likely token
- `mirostatV1()` / `mirostatV2()` — steer output "surprise" toward a target

`minKeep` (on the truncation steps) is the floor on how many tokens survive a cut.

For reproducible output, set the RNG seed with `seed(value)` anywhere in the chain.
It is consumed by every random sampler — `dist`, `mirostatV1`, `mirostatV2`, and the `xtc`
shift step. `greedy` ignores it. If unset, a default seed is used.

```kotlin
val sampler = buildSampler {
    topK(40)
    temperature(0.8)
    seed(42)
    dist()
}
```

You can also change the sampler on an existing chat:

```kotlin
val newSampler = buildSampler {
    temperature(1.2)
    topP(0.9)
    dist()
}
chat.setSamplerConfig(newSampler)
```
