---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_position: 4
---

The model does not produce tokens but rather a probability distribution over all possible tokens. We must then choose how to pick the next token from the distribution. This is the job of a **sampler**, which you can freely configure to achieve better quality outputs or constrain outputs to a known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers well-known presets. For example, to adjust the "creativity" of your model:

```csharp
using NobodyWho;

using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    sampler: SamplerPresets.Temperature(0.2f));
```

Setting `temperature` to `0.2` makes the distribution less flat, so the model favours more probable tokens.

The full list of presets:

```csharp notest
public static class SamplerPresets
{
    public static SamplerConfig Default();
    public static SamplerConfig Dry();
    public static SamplerConfig Greedy();
    public static SamplerConfig Json();
    public static SamplerConfig Temperature(float temperature);
    public static SamplerConfig TopK(int topK);
    public static SamplerConfig TopP(float topP);

    // Constrain output to a specific format:
    public static SamplerConfig ConstrainWithJsonSchema(string schema);
    public static SamplerConfig ConstrainWithRegex(string pattern);
    public static SamplerConfig ConstrainWithGrammar(string grammar);
}
```

All presets have top-k, top-p, temperature and dist steps, and change or add just the one thing
they are named for: the top-k, top-p and temperature presets each override their counterpart,
while the others add a step and leave the three defaults alone. `Greedy()` is the exception —
it always picks the most probable token, so it needs no other steps.

## Structured output

One of the most useful features is constraining the model to produce structured output — this gives you a hard guarantee that the output matches a specific format.

### Regular expressions

For simpler patterns, constrain the output with a regex:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    sampler: SamplerPresets.ConstrainWithRegex("yes|no"));

string answer = await chat.Ask("Is the sky blue?").CompletedAsync();
// answer is guaranteed to be exactly "yes" or "no"
```

### JSON schema

Enforce any JSON output:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    sampler: SamplerPresets.Json());
```

Or use a JSON schema for specific object shapes:

```csharp
using System.Text.Json;
using NobodyWho;

const string schema = """
{
    "type": "object",
    "properties": {
        "name": {"type": "string", "maxLength": 50},
        "age": {"type": "integer"}
    },
    "required": ["name", "age"],
    "additionalProperties": false
}
""";

using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    sampler: SamplerPresets.ConstrainWithJsonSchema(schema));

string response = await chat.Ask("Give me a person with name and age.").CompletedAsync();
// response is always valid JSON matching the schema
using var person = JsonDocument.Parse(response);
Console.WriteLine(person.RootElement.GetProperty("name").GetString());
```

### Custom grammars (advanced)

For cases where JSON schema and regex are not expressive enough, supply a custom grammar. `ConstrainWithGrammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format).

**Lark syntax** (recommended):

```csharp
var sampler = SamplerPresets.ConstrainWithGrammar("""
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,"\n\r]+/
    NEWLINE: /\r?\n/
    """);
```

**GBNF syntax** (also accepted):

```csharp
var sampler = SamplerPresets.ConstrainWithGrammar("""
    file   ::= record (newline record)* newline?
    record ::= field ("," field)*
    field  ::= /[^,"\n\r]+/
    newline ::= "\r\n" | "\n"
    """);
```

## Building custom samplers

Sampler presets abstract away some control. For more advanced configurations — chaining samplers, tuning parameters — use `SamplerBuilder`:

```csharp
SamplerConfig sampler = new SamplerBuilder()
    .TopK(40)
    .Temperature(0.8f)
    .MinP(0.05f)
    .Dist();

using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    sampler: sampler);
```

Each step returns a new builder rather than changing the one it was called on, so you can keep a partly built chain and finish it several ways.

### Available sampling steps

On a `SamplerBuilder`, call any of the **shift steps** below (each reshapes the distribution), then one **terminal step** that picks the token and returns the `SamplerConfig`. Most steps have defaults, so you only pass what you want to change.

Shift steps — call as many as you want, in order:

- `TopK(40)` — keep only the 40 most likely tokens
- `TopP(0.95f)` — nucleus: keep the top tokens up to 95% of the probability mass
- `MinP(0.05f)` — drop tokens below 5% of the most likely token's probability
- `TypicalP(0.9f)` — keep tokens whose "surprise" is close to average, dropping both the too-predictable and the too-random ([locally typical sampling](https://arxiv.org/abs/2202.00666))
- `Xtc(0.5f, 0.1f)` — "exclude top choices": occasionally drop the top tokens for more variety
- `Temperature(0.8f)` — below 1.0 = more focused, above 1.0 = more random
- `DynamicTemperature(temperature: 0.8f, delta: 0.3f, exponent: 1.5f)` — temperature in range [0.5; 1.1], scaled based on confidence level (`exponent` > 1.0 = when uncertain, higher temperature)
- `Penalties(penaltyRepeat: 1.1f)` — per-token repetition penalty (`penaltyRepeat` 1.0 = off)
- `TopNSigma(2.0f)` — keep only the tokens within 2 standard deviations of the most probable token
- `LogitBias(new Dictionary<int, float> { [1] = -1f, [2] = 3f })` — token 1 less probable, token 2 is more probable (`float.NegativeInfinity` bans a token)
- `Dry()` — penalty for repeated *phrases* (its defaults are a good start)
- `Seed(42)` — fix the RNG for reproducible output

The order you call them matters: `Penalties(...)`, `LogitBias(...)` and `Dry(...)` reweigh
whatever distribution reaches them, so put them *before* any grammar/constraining step
if you want them to see the whole vocabulary.

Constraining steps — the same formats as the presets above, but chainable with the rest:

- `ConstrainWithJsonSchema(...)` — output matches a JSON schema, given as a JSON string
- `ConstrainWithRegex(...)` — output matches a regular expression
- `ConstrainWithGrammar(...)` — output matches a grammar, in either Lark or GBNF syntax
- `Json()` — output is a JSON object of any shape

Constraining steps always run **before** the other shift steps, wherever you call them.
This is to avoid the case where a step like `TopK(5)` followed by a constraint could
find that none of the five surviving tokens is valid, leaving nothing to sample and
aborting generation. Both chains below therefore behave identically.

```csharp
var sampler = new SamplerBuilder()
    .ConstrainWithRegex("yes|no")
    .Temperature(0.8f)
    .Dist();

var same = new SamplerBuilder()
    .Temperature(0.8f)
    .ConstrainWithRegex("yes|no")
    .Dist();
```

Terminal step — call exactly one:

- `Dist()` — pick a token with weighted randomness
- `Greedy()` — always take the most likely token
- `MirostatV1()` / `MirostatV2()` — steer output "surprise" toward a target

`minKeep` (on the truncation steps) is the floor on how many tokens survive a cut.

For reproducible output, set the RNG seed with `Seed(value)` anywhere in the chain.
It is consumed by every random sampler — `Dist`, `MirostatV1`, `MirostatV2`, and the `Xtc`
shift step. `Greedy` ignores it. If unset, a default seed is used.

```csharp
var sampler = new SamplerBuilder()
    .TopK(40)
    .Temperature(0.8f)
    .Seed(42)
    .Dist();
```

You can also change the sampler on an existing chat:

```csharp
var newSampler = new SamplerBuilder()
    .Temperature(1.2f)
    .TopP(0.9f)
    .Dist();

await chat.SetSamplerConfigAsync(newSampler);
```

## Saving a sampler

A `SamplerConfig` round-trips through JSON, so you can store it in a settings file. `chat.GetSamplerConfigJsonAsync()` returns the chat's current sampler in the same format:

```csharp
string json = sampler.ToJson();
// ... later ...
SamplerConfig restored = SamplerConfig.FromJson(json);
```
