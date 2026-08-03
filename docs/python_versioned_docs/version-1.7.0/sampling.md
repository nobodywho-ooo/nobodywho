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
```python
from nobodywho import SamplerPresets

Chat("./model.gguf", sampler=SamplerPresets.temperature(0.2))
```
Setting `temperature` to `0.2`, will then affect the sampler when choosing the next token, making the distribution less flat and therefore the model will favour more probable tokens.

To see the whole list of presets, check out the `SamplerPresets` class:
```python
class SamplerPresets:
    def default() -> SamplerConfig: ...
    def dry() -> SamplerConfig: ...
    def greedy() -> SamplerConfig: ...
    def json() -> SamplerConfig: ...
    def temperature(temperature: float) -> SamplerConfig: ...
    def top_k(top_k: int) -> SamplerConfig: ...
    def top_p(top_p: float) -> SamplerConfig: ...

    # Constrain output to a specific format:
    def constrain_with_json_schema(schema: str) -> SamplerConfig: ...
    def constrain_with_regex(pattern: str) -> SamplerConfig: ...
    def constrain_with_grammar(grammar: str) -> SamplerConfig: ...
```

## Structured output

One of the most useful features is constraining the model to produce structured output —
this gives you a hard guarantee that the output matches a specific format, rather than
relying on the model to get it right on its own.

### Regular expressions

For simpler patterns, you can constrain the output with a regex:
```python
# Force the model to answer with exactly "yes" or "no"
chat = Chat('./model.gguf', sampler=SamplerPresets.constrain_with_regex(r"yes|no"))
answer = chat.ask("Is the sky blue?").completed()
```

### JSON schema

In some use-cases it might be useful to let the LLM generate JSON output.
This could be done either in the simple way, just enforcing any JSON by the preset:

```python
Chat('./model.gguf', sampler=SamplerPresets.json())
```

Or utilizing JSON schemas to really force the LLM to give you the specific object shapes
that you want:
```python
import json
chat = Chat('./model.gguf', sampler=SamplerPresets.constrain_with_json_schema({
    "type": "object",
    "properties": {
        "name": {"type": "string", "maxLength": 50},
        "age":  {"type": "integer"}
    },
    "required": ["name", "age"],
    "additionalProperties": False
}))
response = chat.ask("Give me a person as JSON with name and age fields.").completed()
person = json.loads(response)  # always valid JSON matching the schema
```

### Custom grammars

For cases where JSON schema and regex are not expressive enough, you can supply a custom grammar.
`constrain_with_grammar` accepts both **Lark** syntax and **GBNF** (llama.cpp format) -
NobodyWho automatically converts GBNF to Lark before passing it to the inference engine.

**Lark syntax** (recommended):
```python
sampler = SamplerPresets.constrain_with_grammar("""
    start: record (NEWLINE record)* NEWLINE?
    record: field ("," field)*
    field: /[^,\"\\n\\r]+/
    NEWLINE: /\\r?\\n/
""")
```

**GBNF syntax** (also accepted):
```python
sampler = SamplerPresets.constrain_with_grammar("""
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
The older `SamplerPresets.grammar()` method is deprecated. Use `SamplerPresets.constrain_with_grammar()` instead - it accepts both Lark and GBNF strings and should run faster!
:::


## Defining your own samplers

Sampler presets abstract away some control, that you might want - for example, if you
want to chain samplers, change more "advanced" parameters, etc. For that use case,
we provide `SamplerBuilder` class:
```python
from nobodywho import SamplerBuilder

Chat(
    "./model.gguf",
    sampler=SamplerBuilder()
        .temperature(0.8)
        .top_k(5)
        .dist()
)
```
With `SamplerBuilder` you can chain multiple steps together and then select how do you
want to sample from the distribution. Keep in mind, that `SamplerBuilder` provides two
types of methods: ones which modify the distribution (returning again the instance of
`SamplerBuilder`) and ones which sample from the distribution (returning `SamplerConfig`).
So in order to have the sampler working properly and not giving you type errors, be careful
to always end the chain with one of the sampling steps (e.g. `dist`, `greedy`, `mirostat_v2`, etc.).

For reproducible output, set the RNG seed with `.seed(value)` anywhere in the chain.
It is consumed by every random sampler in the chain — `dist`, `mirostat_v1`, `mirostat_v2`,
and the `xtc` shift step. `greedy` ignores it. If unset, a default seed is used.

```python
sampler = SamplerBuilder().temperature(0.8).top_k(5).seed(42).dist()
```

### Available sampling steps

Pick any of the **shift steps** below (each reshapes the token distribution), then finish with one **terminal step** that picks the token — exactly like the `.temperature(0.8).top_k(5).dist()` chain above.

Shift steps — add as many as you want, applied in order:

- `.top_k(40)` — keep only the 40 most likely tokens
- `.top_p(0.95, min_keep=1)` — nucleus: keep the top tokens up to 95% of the probability mass
- `.min_p(0.05, min_keep=1)` — drop tokens below 5% of the most likely token's probability
- `.typical_p(0.9, min_keep=1)` — keep tokens whose "surprise" is close to average, dropping both the too-predictable and the too-random ([locally typical sampling](https://arxiv.org/abs/2202.00666))
- `.xtc(0.5, 0.1, min_keep=1)` — "exclude top choices": occasionally drop the top tokens for more variety
- `.temperature(0.8)` — below 1.0 = more focused, above 1.0 = more random
- `.penalties(64, 1.1, 0.0, 0.0)` — per-token repetition penalty: `last_n, repeat, freq, present` (`repeat` 1.0 = off)
- `.dry(0.8, 1.75, 2, -1, ["\n"])` — penalty for repeated *phrases*: `multiplier, base, allowed_length, last_n, seq_breakers`
- `.seed(42)` — fix the RNG for reproducible output
- `.grammar(...)` — deprecated; use the `constrain_with_*` presets above

Terminal step — one of these turns the chain into a `SamplerConfig`, so finish with exactly one:

- `.dist()` — pick a token with weighted randomness (the usual choice)
- `.greedy()` — always take the most likely token
- `.mirostat_v1(5.0, 0.1, 100)` / `.mirostat_v2(5.0, 0.1)` — steer output "surprise" toward a target

`min_keep` is the floor on how many tokens survive a cut (`1` is fine).

