---
title: Basics
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Basics
order: 1
---

As you may noticed in the [welcome guide](./index.md), every interaction with your LLM starts by instantiating a `Chat` object.
In the following sections, we talk about which configuration options is has, and when to use them.

## Prompts and responses

The `Chat.ask()` function is central to NobodyWho. It is so named because it lets you say something to the LLM, and starts generating a response in return.

```python
from nobodywho import Chat, TokenStream
chat = Chat("./model.gguf")
response: TokenStream = chat.ask("Is water wet?")
```

The return type of `ask` is a `TokenStream`.
If you want to start reading the response as soon as possible, you can just iterate over the `TokenStream`.
Each token is either an individual word or fragments of a word.

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

## Chat history

If you want to inspect the messages inside the `Chat` object, you can use `get_chat_history`.

```{.python continuation}
msgs: list[dict] = chat.get_chat_history()
print(msgs[0]["content"]) # "Is water wet?"
```

Similarly, if you want to edit what messages are in the context, you can use `set_chat_history`:


```{.python continuation}
chat.set_chat_history([{"role": "user", "content": "What is water?"}])
```

## System prompt

A system prompt is a special message put into the chat context, which should guide its overall behavior.
Some models ship with a built-in system prompt. If you don't specify a system prompt yourself, NobodyWho will fall back to using the model's default system prompt.

You can specify a system prompt when initializing a `Chat`:

```python
from nobodywho import Chat
chat = Chat("./model.gguf", system_prompt="You are a mischievous assistant!")
```

This `system_prompt` is then persisted until the chat context is `reset`.



## Context

The context is the text window which the LLM currently considers. Specifically this is the number of tokens the LLM keeps in memory and you can view it as the LLM's memory of your current conversation.
As bigger context size means more computational overhead, it makes sense to constrain it. This can be done with `n_ctx` setting, again at the time of creation:

```python
chat = Chat("./model.gguf", n_ctx=4096)
```

The default value is `2048`, however this is mainly usefull for short and simple conversations. Choosing the right context size is quite important and depends heavily on your use case. A good place to start is to look at your selected models documentation and see what their recommended context size is.<br> Even with properly selected context size it might happen that you fill up your entire context during a conversation. When this happens, NobodyWho will shrink the context for you. Currently this is done by removing old messages (apart from the system prompt and the first user message) from the chat history,
until the size reaches `n_ctx / 2`. The KV cache is also updated automatically. In the future we plan on adding more advanced methods of context shrinking.

Again, `n_ctx` is fixed to the `Chat` instance, so it is currently not possible to change the size after `Chat` is created. To reset the current context content, just call `.reset()` with the new system prompt and potentionally changed tools.

```{.python continuation}
chat.reset(system_prompt="New system prompt", tools=[])
```

If you don't want to change the already set defaults (`system_prompt`, `tools`), but only reset the context, then go for `reset_history`.

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
Note that `Chat` has thinking set to `True` by default. Note that you can also change the setting
of an already existing chat instance:
```{.python continuation}
chat.set_allow_thinking(True)
```
With the next message send, the setting will be propagated.

!!! info ""
    Note that **not every model** supports reasoning. If the model does not have
    such an option, the `allow_thinking` setting will be ignored.
