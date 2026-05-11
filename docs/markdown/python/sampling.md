---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_title: Sampling
order: 4
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
chat = Chat('./model.gguf', sampler=SamplerPresets.constrain_with_json_schema({
    "type": "object",
    "properties": {
        "name": {"type": "string"},
        "age":  {"type": "integer"}
    },
    "required": ["name", "age"]
}))
response = chat.ask("Give me a person.").completed()
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

!!! info ""
    The older `SamplerPresets.grammar()` method is deprecated. Use `SamplerPresets.constrain_with_grammar()` instead - it accepts both Lark and GBNF strings and should run faster!


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

