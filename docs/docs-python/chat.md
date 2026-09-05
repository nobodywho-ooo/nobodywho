---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_position: 1
---

As you may have noticed in the [welcome guide](./), every interaction with your LLM starts by instantiating a `Chat` object.
In the following sections, we talk about which configuration options it has, and when to use them.

You can pass `"auto"` as the model path to select a chat model based on available memory.

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

```python continuation
for token in response:
   print(token, end="", flush=True)
print("\n")
```

If you just want to get the complete response, you can call `TokenStream.completed()`.
This will block until the model is done generating its entire response.

```python continuation
full_response: str = response.completed()
```

All of your messages and the model's responses are stored in the `Chat` object, so the next time you call `Chat.ask()`, it will remember the previous messages.

## Chat history

If you want to inspect the messages inside the `Chat` object, you can use `get_chat_history`.

```python continuation
msgs: list[dict] = chat.get_chat_history()
print(msgs[0]["content"]) # "Is water wet?"
```

Similarly, if you want to edit what messages are in the context, you can use `set_chat_history`:


```python continuation
chat.set_chat_history([{
   "role": "user",
   "content": "What is water?"
}])
```

## Chat completion

If you are used to other LLM libraries, you may prefer passing the whole conversation on every call instead of letting the `Chat` object remember it. That is what `Chat.complete()` is for:

```python
from nobodywho import Chat, TokenStream
chat = Chat("./model.gguf")
response: TokenStream = chat.complete([
   {"role": "system", "content": "You are a helpful assistant."},
   {"role": "user", "content": "Who was the first person to walk on the moon?"},
   {"role": "assistant", "content": "Neil Armstrong."},
   {"role": "user", "content": "Which year did he do it?"},
])
print(response.completed())
```

You get back the same `TokenStream` as from `ask()`, so you can iterate it for tokens or call `completed()` for the whole answer.

The list you pass **becomes** the chat history, replacing whatever was there. The response is added to it, so `ask()` continues that same conversation:

```python continuation
chat.complete([{"role": "user", "content": "My favorite color is teal."}]).completed()

print(chat.get_chat_history())  # the message about the color, plus the reply
print(chat.ask("What is my favorite color?").completed())  # continues from there
```

A system message at the front sets the chat's system prompt. If you leave it out, the prompt already on the chat is kept:

```python continuation
chat = Chat("./model.gguf", system_prompt="You are a helpful assistant.")
chat.complete([{"role": "user", "content": "Hello!"}]).completed()
print(chat.get_system_prompt())  # "You are a helpful assistant." — kept

chat.complete([
   {"role": "system", "content": "You are a pirate."},
   {"role": "user", "content": "Hello!"},
]).completed()
print(chat.get_system_prompt())  # "You are a pirate." — replaced
```

The list has to describe a conversation the model can answer, so it must not be empty, it must end in a user or tool message, and only the first message may be a system message. Anything else raises a `ValueError`:

```python continuation
try:
   chat.complete([
      {"role": "user", "content": "Was the cat a tabby?"},
      {"role": "assistant", "content": "Aye, "},  # nothing left to answer
   ])
except ValueError as e:
   print(e)
```

Use `ask()` to add a single turn to the conversation the `Chat` is already holding, and `complete()` to hand it a conversation of your own. Both leave the chat ready for the other.

### Per-turn settings

`complete()` also takes the chat's other settings as keyword arguments — `sampler`, `template_variables` and `tools`. They follow the same rule as the system message: what you pass stays set, what you leave out is kept.

```python continuation
from nobodywho import SamplerPresets

chat.complete(
   [{"role": "user", "content": "Name one fruit."}],
   sampler=SamplerPresets.greedy(),
   template_variables={"enable_thinking": False},
).completed()

# Both are now the chat's settings, so the next call need not repeat them
print(chat.get_template_variables())  # {'enable_thinking': False}
chat.complete([{"role": "user", "content": "Name another."}]).completed()
print(chat.get_template_variables())  # {'enable_thinking': False} — still
```

Pass all three and the call no longer depends on what the chat is currently holding, which is what you want if you are driving it entirely through `complete()`.

Changing `tools` re-selects the chat template and rewrites the system-prompt region, so that turn re-prefills from near token zero. It is the one option with a real cost attached — set it when it changes, not on every call.

### Structured results

The structured methods use field names from the [OpenAI Chat Completions API](https://developers.openai.com/api/reference/resources/chat/subresources/completions/create) and [Responses API](https://developers.openai.com/api/reference/resources/responses/methods/create). They return NobodyWho classes rather than OpenAI SDK objects. The classes support attribute access, item access, `to_dict()`, and `model_dump()`.

Pass `stream=False` to return an OpenAI-shaped completion instead of the legacy `TokenStream`:

```python continuation
completion = chat.complete(
   [{"role": "user", "content": "Name one fruit."}],
   sampler=SamplerPresets.greedy(),
   thinking=False,
   max_tokens=64,
   stream=False,
)

print(completion.choices[0].message.content)
print(completion["usage"])
```

Pass `stream=True` for OpenAI-shaped chunks. The stream's `completed()` method returns the final structured completion:

```python continuation
stream = chat.complete(
   [{"role": "user", "content": "Name one fruit."}],
   stream=True,
)

for chunk in stream:
   content = chunk.choices[0].delta.content
   if content:
      print(content, end="")

completion = stream.completed()
```

Leaving `stream` out keeps the existing behavior and returns a `TokenStream` of strings.

Use `temperature`, `top_p`, and `seed` when porting an OpenAI request. These values apply only to that request and do not change the sampler stored on the chat:

```python continuation
completion = chat.complete(
   messages=[{"role": "user", "content": "Name one fruit."}],
   temperature=0.7,
   top_p=0.9,
   seed=42,
   max_completion_tokens=64,
   stream=False,
)
```

`max_completion_tokens` is an alias for `max_tokens`. Do not pass both names. A direct sampling argument cannot be combined with `sampler`. Request-only sampling arguments and token limits require an explicit `stream=True` or `stream=False`; omitting `stream` keeps the legacy behavior.

### Responses-style input

`respond()` accepts a string or an explicit list of messages. It returns a structured Responses-style result by default:

```python continuation
response = chat.respond(
   instructions="Answer concisely.",
   input="Name one fruit.",
   sampler=SamplerPresets.greedy(),
   thinking=False,
   max_output_tokens=64,
)

print(response.output_text)
print(response.output)
print(response.usage)
```

`respond()` also accepts request-only `temperature`, `top_p`, and `seed` arguments. It uses `max_output_tokens` for its token limit.

Pass `stream=True` to receive `response.output_text.delta` events followed by a `response.completed` event. This is a small subset of the OpenAI Responses event protocol. Tools passed to `complete()` or `respond()` use NobodyWho's existing tool loop and run locally. Unknown request arguments, JSON schema output, and configurable context-overflow policies are not supported.

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

The default value is `4096`, however this is mainly useful for short and simple conversations. Choosing the right context size is quite important and depends heavily on your use case. You can check the maximum context size the model was trained with using `model.max_ctx()` — setting `n_ctx` above this value has no benefit.

Even with properly selected context size it might happen that you fill up your entire context during a conversation. When this happens, NobodyWho will shrink the context for you. Currently this is done by removing old messages (apart from the system prompt and the first user message) from the chat history, until the size reaches `n_ctx / 2`. The KV cache is also updated automatically. In the future we plan on adding more advanced methods of context shrinking.

Again, `n_ctx` is fixed to the `Chat` instance, so it is currently not possible to change the size after `Chat` is created. To reset the current context content, just call `.reset()` with the new system prompt and potentially changed tools.

```python continuation
chat.reset(system_prompt="New system prompt", tools=[])
```

If you don't want to change the already set defaults (`system_prompt`, `tools`), but only reset the context, then go for `reset_history`.

To inspect how much of the context is currently in use, call `.stats()`:

```python continuation
stats = chat.stats()
print(f"Using {stats.context_used} of {stats.context_size} tokens")
```

## CPU threads

When layers run on the CPU, NobodyWho picks a thread count for you: one per *performance* core,
not one per logical CPU. Hyperthread siblings and efficiency cores end up pacing the whole
thread pool, so using every CPU is usually slower — see [LLM Basics](/docs/llm-basics#cpu-threads)
for the numbers.

Override it with `n_threads` when you want to leave CPU headroom for other work, or when
NobodyWho could not read your machine's topology (it logs a warning if so):

```python
chat = Chat("./model.gguf", n_threads=4)
```

Leave it unset — or pass `None` — to keep the detected default. Values above your CPU count are
clamped, and it has little effect when the model is offloaded to the GPU.

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

## Speculative decoding (MTP)

Some models come with **MTP** (Multi-Token Prediction) draft heads that let the target model verify several candidate tokens per forward pass. When it works this can give a significant speedup — but see the warning below before enabling it.

To use MTP, load the model with a compatible draft-heads gguf (e.g. `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B) and pass an `MtpConfig` when constructing the chat:

```python notest
from nobodywho import Chat, Model, MtpConfig

model = Model("./gemma-4-e2b.gguf", draft_model_path="./mtp-gemma-4-e2b.gguf")
# MtpConfig() uses the default drafter tuning; tune with MtpConfig(k_max=..., p_min=...)
chat = Chat(model, mtp=MtpConfig())
```

Loading the draft heads adds around 5% to VRAM usage. See [LLM Basics](/docs/llm-basics#speculative-decoding-mtp) for the underlying idea.

:::warning
Benchmark before enabling. MTP can hurt performance on Apple Silicon (Metal) and on high-entropy workloads like creative prose.
:::

## Template Variables

Chat templates are used internally by models to format conversation history into the expected prompt format. Different models may support different template variables that control specific behaviors. Template variables are boolean flags passed to the chat template that can enable or disable certain features.

### Using Template Variables

You can set template variables when creating a chat or modify them on existing instances:

```python
# Set template variables when creating a chat
chat = Chat("./model.gguf", template_variables={"enable_thinking": True})
```

You can also modify template variables on an existing chat instance:

```python continuation
# Set a single template variable
chat.set_template_variable("enable_thinking", True)

# Set multiple template variables at once
chat.set_template_variables({
    "enable_thinking": True,
    "verbose_mode": False
})

# Get current template variables
variables = chat.get_template_variables()
print(variables)  # {"enable_thinking": True, "verbose_mode": False}
```

With the next message sent, the updated settings will be propagated to the model.

### Example: Qwen3 and Qwen3.5 Reasoning

The Qwen3 and Qwen3.5 model families support the `enable_thinking` template variable, which controls whether the model should engage in explicit reasoning steps before answering:

```python
# Enable thinking mode for Qwen models
chat = Chat("./model.gguf", template_variables={"enable_thinking": True})
chat.ask("Solve this logic puzzle: ...")
```

When `enable_thinking` is enabled, these models will show their reasoning process before providing the final answer.

### Model-Specific Variables

Different models may support different template variables depending on their chat template implementation. The available variables and their effects depend entirely on how the model's chat template is designed. Check your model's documentation to see which template variables are supported.

:::info
Note that template variables are model-specific. If a model's chat template doesn't use a specific variable, that variable will be ignored gracefully.
:::

### Backward Compatibility

For backward compatibility, the deprecated `allow_thinking` parameter is still available but internally sets the `enable_thinking` template variable:

```python
# Deprecated - use template_variables instead
chat = Chat("./model.gguf", allow_thinking=True)
chat.set_allow_thinking(True)
```
