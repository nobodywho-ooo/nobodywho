---
title: Basics
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Basics
order: 1
---

As you may noticed in the [welcome guide](./index.md), every interaction with your LLM starts by instantiating a `Chat` object.
In the following sections, we talk about which configuration options does it have, and when to use them.

## Prompts and responses

The `Chat.ask()` function is central to NobodyWho. It is so named because it lets you say something to the LLM, and starts generating a response in return.

```python
from nobodywho import Chat, TokenStream
chat = Chat("./model.gguf")
response: TokenStream = chat.ask("Is water wet?")
```

The return type of `ask` is a `TokenStream`.
If you want to start reading the response as soon as possible, you can just iterate over the `TokenStream`.
The individual tokens are either individual words or fragments of words.

```{.python continuation}
for token in response:
   print(token, end="")
print("\n")
```

If you just want to get the complete response, you can call `TokenStream.completed()`.
This will block until the model is done generating its entire response.

```{.python continuation}
full_response: str = response.completed()
```

All of your messages and the model's responses are stored in the `Chat` object, so the next time you call `Chat.ask()`, it will remember the previous messages.

## System prompt

Every LLM usually has some sort of system prompt - an instruction passed to the model,
which should specify an overall behaviour through the `Chat` context. System prompts
are either provided by the creator of the model (in which case NobodyWho opts for this system prompt),
or an empty string is selected. System prompt can be also set manually, when creating the chat:

```python
from nobodywho import Chat
chat = Chat("./model.gguf", system_prompt="You are a mischievous assistant!")
```
This `system_prompt` is then persisted until the chat context is `reset`.

## Context

The context is the text window which the LLM currently considers - you can see it as a limited-length chat history.
As bigger context size means more computational overhead, it makes sense to constrain it. This can be done with `n_ctx` setting, again at the time of creation:

```python
chat = Chat("./model.gguf", n_ctx=4096)
```

The default value is `2048`. When text is about to overflow away from the context, NobodyWho automatically crops
the first messages apart from the system prompt and first user message (as these usually bear the most context),
until the size reaches `n_ctx / 2`. KV cache is also updated automatically.

Again, `n_ctx` is fixed to the `Chat` instance, so it is currently not possible to change the size after `Chat` is created. To reset the current context content, just call `.reset()` with the new system prompt and potentionally changed tools.

```{.python continuation}
chat.reset(system_prompt="New system prompt", tools=[])
```

<div style="background-color: red;">
    TODO: Enable smoother reset, with defaults. This is clunky.
</div>

## Sharing model between contexts

There are scenarios, where you would like to keep separate chat contexts (e.g. for every user of your app), but have only one model loaded. With plain `Chat` this is not possible.

For this use case, instead of the path to the `.gguf` model, you can pass in `Model` object, which can be shared between multiple `Chat` instances.

```python
from nobodywho import Model

model = Model('./model.gguf')
chat1 = Chat(model)
chat2 = Chat(model)
...
```

NobodyWho will then take care of the separation, such that your chat histories won't collide or interfere with each other, while having only one model loaded.

## GPU
Instantiating `Model` is also useful, when enabling GPU acceleration. This can be done as:
```python
Model('./model.gguf', use_gpu_if_available=True)
```
So far, NobodyWho relies purely on [Vulkan](https://www.vulkan.org), however support
of more architectures is planned (for details check out our [issues](https://github.com/nobodywho-ooo/nobodywho/issues) or join us on [Discord](https://discord.gg/qhaMc2qCYB)).

## Reasoning

To enhance the cognitive performance, allowing reasoning can be a good idea:
```python
chat = Chat("./model.gguf", allow_thinking=True)
```
Note that `Chat` has thinking set to `True` by default. This time, you can also change the setting
with already existing chat instance:
```{.python continuation}
chat.set_allow_thinking(True)
```
With the next message send, the setting will be propagated.

!!! info ""
    Note that **not every model** supports reasoning. If the model does not have
    such an option, the `allow_thinking` setting will be ignored.
