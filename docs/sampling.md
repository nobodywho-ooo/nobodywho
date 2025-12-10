---
title: Sampling
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Sampling
order: 3
---

As you may know, current transformer-based LLMs are "just" clever next word prediction machines (also called auto-regressive). Since these next word predictions come not as a fixed word, but a probability distribution, we can choose how to pick the next word from the distribution. This is the job of a **sampler**, which using NobodyWho you can freely modify,
to achieve better quality outputs or constrain the outputs to some known format (e.g. JSON).

## Sampler presets

To get a quick start, NobodyWho offers a couple of well-known presets, which you can quickly utilize.
For example, if you want to increase or decrease the "creativity" of your model, select our `temperature` preset:
```python
from nobodywho import SamplerPresets

Chat("./model.gguf", sampler=SamplerPresets.temperature(0.2))
```
Setting `temperature` to `0.2`, will then affect the sampler when choosing the next word, making the distribution less flat and therefore the model will favour more probable words.

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
For a nice specification, [head over to this site](https://github.com/ggml-org/llama.cpp/blob/master/grammars/README.md).

<div style="background-color: red;">
    TODO: Enable importing grammar from JSON.
</div>

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
types of methods: these which modify the distribution (returning again the instance of
`SamplerBuilder`) and these which sample from the distribution (returning `SamplerConfig`).
So in order to have the sampler working properly and not giving you type errors, be careful
to always end the chain with some of the sampling steps (e.g. `dist`, `greedy`, `mirostat_v2`, etc.).

