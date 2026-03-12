---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_title: Chat
order: 1
---

As you may have noticed in the [welcome guide](./index.md), every interaction with your LLM starts by instantiating a `Chat` object.
In the following sections, we talk about which configuration options it has, and when to use them.

## Prompts and responses

The `Chat.ask()` function is central to NobodyWho. This function sends your message to the LLM, which then starts generating a response.

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
   print(token, end="", flush=True)
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
chat.set_chat_history([{
   "role": "user",
   "content": "What is water?",
   "assets": []
}])
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

The context is the text window which the LLM currently considers. Specifically this is the number of tokens the LLM keeps in memory for your current conversation.
As bigger context size means more computational overhead, it makes sense to constrain it. This can be done with `n_ctx` setting, again at the time of creation:

```python
chat = Chat("./model.gguf", n_ctx=4096)
```

The default value is `4096`, however this is mainly useful for short and simple conversations. Choosing the right context size is quite important and depends heavily on your use case. A good place to start is to look at your selected models documentation and see what their recommended context size is.

Even with properly selected context size it might happen that you fill up your entire context during a conversation. When this happens, NobodyWho will shrink the context for you. Currently this is done by removing old messages (apart from the system prompt and the first user message) from the chat history, until the size reaches `n_ctx / 2`. The KV cache is also updated automatically. In the future we plan on adding more advanced methods of context shrinking.

Again, `n_ctx` is fixed to the `Chat` instance, so it is currently not possible to change the size after `Chat` is created. To reset the current context content, just call `.reset()` with the new system prompt and potentially changed tools.

```{.python continuation}
chat.reset(system_prompt="New system prompt", tools=[])
```

If you don't want to change the already set defaults (`system_prompt`, `tools`), but only reset the context, then go for `reset_history`.

## Sharing model between contexts

There are scenarios where you would like to keep separate chat contexts (e.g. for every user of your app), but have only one model loaded. With plain `Chat` this is not possible.

For this use case, instead of the path to the `.gguf` model, you can pass in `Model` object, which can be shared between multiple `Chat` instances.

```python
from nobodywho import Chat, Model

model = Model('./model.gguf')
chat1 = Chat(model)
chat2 = Chat(model)
...
```

NobodyWho will then take care of the separation, such that your chat histories won't collide or interfere with each other, while having only one model loaded.

## Asynchronous model loading

Loading a model into memory can take a few seconds - longer if you're using a really large model.

If you want to load the model without blocking execution of your application (e.g. to keep UI responsive), you can load the model asynchronously:


```python
import asyncio
from nobodywho import ChatAsync, Model

async def main():
   model = await Model.load_model_async("./model.gguf")
   chat = ChatAsync(model)

asyncio.run(main())
```

## GPU
Instantiating `Model` is also useful, when enabling GPU acceleration. This can be done as:
```python
Model('./model.gguf', use_gpu_if_available=True)
```
So far, NobodyWho relies purely on [Vulkan](https://www.vulkan.org), however support
of more architectures is planned (for details check out our [issues](https://github.com/nobodywho-ooo/nobodywho/issues) or join us on [Discord](https://discord.gg/qhaMc2qCYB)).

## Reasoning

To enhance cognitive performance, allowing reasoning can be a good idea:
```python
chat = Chat("./model.gguf", allow_thinking=True)
```
Note that `Chat` has thinking set to `True` by default. You can also change the setting
of an already existing chat instance:
```{.python continuation}
chat.set_allow_thinking(True)
```
With the next message send, the setting will be propagated.

!!! info ""
    Note that **not every model** supports reasoning. If the model does not have
    such an option, the `allow_thinking` setting will be ignored.
