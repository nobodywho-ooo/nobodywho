---
title: Sampling
description: A description of how samplers can be configured in NobodyWho
sidebar_title: Sampling
order: 3
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
    def grammar(grammar: str) -> SamplerConfig: ...
    def greedy() -> SamplerConfig: ...
    def json() -> SamplerConfig: ...
    def temperature(temperature: float) -> SamplerConfig: ...
    def top_k(top_k: int) -> SamplerConfig: ...
    def top_p(top_p: float) -> SamplerConfig: ...
    ...
```

## Structured output

One of the most useful presets to have, is to be able to generate structured output,
such as JSON. This way, you dont have to rely on your model being clever enough to
generate syntactically valid JSON, but instead you are strictly guaranteed that the
output will be right. For plain JSON, it suffices to:
```python
Chat('./model.gguf', sampler=SamplerPresets.json())
```

Still, you might have more advanced needs, such as generating CSVs or JSON with some specific keys. This can be supported by creating custom grammars, such as this one for CSV:
```python
sampler = SamplerPresets.grammar("""
    file ::= record (newline record)* newline?
    record ::= field ("," field)*
    field ::= quoted_field | unquoted_field
    unquoted_field ::= unquoted_char*
    unquoted_char ::= [^,"\n\r]
    quoted_field ::= "\"" quoted_char* "\""
    quoted_char ::= [^"] | "\"\""
    newline ::= "\r\n" | "\n"
""")
```
The format that NobodyWho utilizes is called GBNF, which is a Llama.cpp native format.
See the [GBNF specification](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md).


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

