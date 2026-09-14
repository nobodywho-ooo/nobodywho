---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_position: 4
---

The model does not produce tokens but rather a probability distribution over all possible tokens.
We must then choose how to pick the next token from the distribution. This is the job of a
**sampler**, which using NobodyWho you can freely modify, to achieve better quality outputs or
constrain the outputs to some known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers a couple of well-known presets. Presets start with the
default top-k, top-p, temperature, and distribution steps, then override or add the behavior they
are named for. `greedy()` is the exception: it always chooses the most probable token and needs no
other steps. For example, to adjust the model's "creativity", select the `temperature` preset:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {
    "sampler": NobodyWhoSamplerPresets.temperature(0.2),
})
```

Setting `temperature` to `0.2` will then affect the sampler when choosing the next token, making
the distribution less flat and therefore the model will favour more probable tokens — good for
deterministic NPC dialogue.

The presets, as static functions on `NobodyWhoSamplerPresets`:

```gdscript
NobodyWhoSamplerPresets.default()
NobodyWhoSamplerPresets.dry()
NobodyWhoSamplerPresets.greedy()
NobodyWhoSamplerPresets.json()
NobodyWhoSamplerPresets.temperature(temperature)
NobodyWhoSamplerPresets.top_k(top_k)
NobodyWhoSamplerPresets.top_p(top_p)

# Constrain output to a specific format:
NobodyWhoSamplerPresets.constrain_with_json_schema(schema)
NobodyWhoSamplerPresets.constrain_with_regex(pattern)
NobodyWhoSamplerPresets.constrain_with_grammar(grammar)
```

## Structured output

One of the most useful features is constraining the model to produce structured output — this
gives you a hard guarantee that the output matches a specific format, rather than relying on the
model to get it right on its own.

### Regular expressions

For simpler patterns, you can constrain the output with a regex:

```gdscript
# Force the model to answer with exactly "yes" or "no"
var chat = await NobodyWhoChat.create("./model.gguf", {
    "sampler": NobodyWhoSamplerPresets.constrain_with_regex("yes|no"),
})
var answer: String = await chat.ask("Is the sky blue?").completed()
```

### JSON schema

In some use-cases it might be useful to let the LLM generate JSON output. Use
`constrain_with_json_schema` to enforce a specific JSON shape:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {
    "sampler": NobodyWhoSamplerPresets.constrain_with_json_schema(JSON.stringify({
        "type": "object",
        "properties": {
            "name": {"type": "string", "maxLength": 50},
            "age": {"type": "integer"},
        },
        "required": ["name", "age"],
        "additionalProperties": false,
    })),
})
var response: String = await chat.ask("Give me a person as JSON with name and age fields.").completed()
var person = JSON.parse_string(response) # always valid JSON matching the schema
```

### Custom grammars (advanced)

For cases where JSON schema and regex are not expressive enough, you can supply a custom
grammar. `constrain_with_grammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format) —
NobodyWho automatically converts GBNF to Lark before passing it to the inference engine.

```gdscript
var sampler = NobodyWhoSamplerPresets.constrain_with_grammar("""
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,"\\n\\r]+/
    NEWLINE: /\\r?\\n/
""")
```

## Defining your own samplers

For full control, chain steps on a `NobodyWhoSamplerBuilder`. Shift steps filter and scale the
token distribution; the chain ends with exactly one sampling step, which picks the token:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {
    "sampler": NobodyWhoSamplerBuilder.new()
        .top_k(40)
        .top_p(0.95, 1)
        .temperature(0.6)
        .dist(),
})
```

Constraints can also be chained into the builder — they always run *before* the other steps,
wherever you put them in the chain, so a constraint can't be starved by an earlier filter:

```gdscript
var cfg = NobodyWhoSamplerBuilder.new()
    .json_schema(schema_dict)
    .temperature(0.8)
    .dist()
```

### Available sampling steps

Shift steps (chainable, each returns a new builder):

| Step | Effect |
| --- | --- |
| `top_k(k)` | Keep only the `k` most probable tokens. Typical: 40-50. |
| `top_p(p, min_keep)` | Keep tokens whose cumulative probability is below `p`. Typical: 0.9-0.95. |
| `min_p(p, min_keep)` | Keep tokens with probability above `p` times the most-likely one. |
| `typical_p(p, min_keep)` | Typical-p sampling. |
| `xtc(probability, threshold, min_keep)` | Probabilistically exclude high-probability tokens for diversity. |
| `temperature(t)` | Scale the distribution. `0` deterministic, `1` unchanged, `>1` more random. |
| `dynamic_temperature(t, delta, exponent)` | Entropy-based temperature: lands in `[t - delta; t + delta]`, computed as `entropy^exponent`. |
| `top_n_sigma(n)` | Keep tokens within `n` standard deviations from the mean. |
| `logit_bias(biases)` | Boost/ban specific token IDs. `biases` is a Dictionary of token ID (int) to bias (float); `-INF` bans a token. |
| `penalties(last_n, repeat, freq, present)` | Repetition/frequency/presence penalties. |
| `dry(multiplier, base, allowed_length, last_n, seq_breakers)` | DRY (Don't Repeat Yourself) penalty. |
| `json_schema(schema)` / `regex(pattern)` / `lark(grammar)` / `json()` | Constrain output (see above). |
| `seed(s)` | RNG seed for random samplers. |

Sampling steps (terminals, return a `NobodyWhoSamplerConfig`):

| Step | Effect |
| --- | --- |
| `dist()` | Weighted random sampling from the distribution. |
| `greedy()` | Always pick the most probable token (deterministic). |
| `mirostat_v1(tau, eta, m)` | Perplexity-controlled sampling. |
| `mirostat_v2(tau, eta)` | Perplexity-controlled sampling, simplified. |

## Persisting a sampler

A finished `NobodyWhoSamplerConfig` can be serialized for save files and settings screens:

```gdscript
var cfg = NobodyWhoSamplerPresets.temperature(0.7)
var json: String = cfg.to_json()
var restored = NobodyWhoSamplerConfig.from_json(json)
```

You can also read and swap the sampler of a live chat:

```gdscript
var current = await chat.get_sampler_config()
await chat.set_sampler_config(NobodyWhoSamplerPresets.greedy())
```
