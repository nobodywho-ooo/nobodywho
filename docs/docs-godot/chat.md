---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_position: 1
---

As you may have noticed in the [welcome guide](./), every interaction with your LLM starts by
creating a `NobodyWhoChat` object. In the following sections, we talk about which configuration
options it has, and when to use them.

## Creating a chat

There are two main ways of creating a chat and the difference lies in when the model file is
loaded. The simplest way is passing a model path to `NobodyWhoChat.create`:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {})
```

`create` is async since loading a model can take a bit of time, but it never blocks your game —
it resolves once the model is ready, or to `null` on failure (with details in the editor's Errors
tab).

Another way is to load the model separately with `NobodyWhoModel.create` and pass that instead:

```gdscript
var model = await NobodyWhoModel.create("./model.gguf", {})
var chat = await NobodyWhoChat.create(model, {})
```

This allows for sharing the model between several chats.

You can pass `"auto"` as the model path to select a chat model based on available memory.

## Prompts and responses

The `ask()` function is central to NobodyWho. It sends your message to the LLM, which then starts
generating a response.

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {})
var stream = chat.ask("Is water wet?")
```

The return type of `ask` is a `NobodyWhoTokenStream`. If you want to start reading the response as
soon as possible, pull tokens one at a time. Each token is either an individual word or a fragment
of a word:

```gdscript
while true:
    var token = await stream.next_token()
    if token == null:
        break
    print(token)
```

`next_token()` resolves to the next token as soon as it exists, or `null` once generation is
done. If you just want the complete response, call `completed()` instead:

```gdscript
var full_response: String = await stream.completed()
```

To stop a generation early — the player closed the dialogue window, say — call
`stop_generation()`:

```gdscript
chat.stop_generation()
```

All of your messages and the model's responses are stored in the chat object, so the next time
you call `ask()`, it remembers the previous messages.

## Chat history

If you want to inspect the messages inside the chat object, use `get_chat_history()`:

```gdscript
var msgs = await chat.get_chat_history()
print(msgs[0]["content"]) # "Is water wet?"
```

Each message is a dictionary with a `"role"` (`"user"`, `"assistant"`, `"system"`, or `"tool"`)
and its `"content"`. You can replace the whole history with `set_chat_history()` — useful for
seeding a conversation or restoring a saved one:

```gdscript
await chat.set_chat_history([
    {"role": "user", "content": "What is water?"},
    {"role": "assistant", "content": "A transparent liquid!"},
])
```

A leading system message sets the chat's system prompt. The list must not be empty, must end in
a user or tool message, and may only have a system message first. To clear the conversation but
keep the system prompt and tools, use `reset_history()`.

## System prompt

A system prompt is a special message put into the chat context, which should guide its overall
behavior. Some models ship with a built-in system prompt. If you don't specify a system prompt
yourself, NobodyWho will fall back to using the model's default system prompt.

You can specify a system prompt when creating the chat:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {
    "system_prompt": "You are a mischievous assistant!",
})
```

Or change it on a live chat — pass `null` to clear it:

```gdscript
await chat.set_system_prompt("You are a grumpy dwarf.")
print(await chat.get_system_prompt())
```

## Context

The context is the text window which the LLM currently considers. Specifically this is the number
of tokens the LLM keeps in memory for your current conversation. A bigger context size means more
computational overhead, so it makes sense to constrain it. This can be done with the `n_ctx`
setting at the time of creation:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {"n_ctx": 4096})
```

The default value is `4096`, however this is mainly useful for short and simple conversations.
Choosing the right context size is quite important and depends heavily on your use case. Setting
`n_ctx` above the maximum context size the model was trained with has no benefit.

Even with a properly selected context size it might happen that you fill up the entire context
during a conversation. When this happens, NobodyWho will shrink the context for you. Currently
this is done by removing old messages (apart from the system prompt and the first user message)
from the chat history, until the size reaches `n_ctx / 2`. The KV cache is also updated
automatically. In the future we plan on adding more advanced methods of context shrinking.

`n_ctx` is fixed to the chat instance. To reset the current context content, call
`reset_history()` (keeps the system prompt and tools), or `reset_chat()` to also change those:

```gdscript
await chat.reset_history()
await chat.reset_chat("New system prompt", [])
```

To inspect how much of the context is currently in use, call `get_stats()`:

```gdscript
var stats = await chat.get_stats()
print("Using %d of %d tokens" % [stats["context_used"], stats["context_size"]])
```

You can also count tokens without generating anything, with `tokenize()`:

```gdscript
var tokens = await chat.tokenize("How many tokens is this?")
print(tokens.size())
```

## CPU threads

When layers run on the CPU, NobodyWho picks a thread count for you: one per *performance* core,
not one per logical CPU. Hyperthread siblings and efficiency cores end up pacing the whole thread
pool, so using every CPU is usually slower — see [LLM Basics](/docs/llm-basics#cpu-threads) for
the numbers.

Override it with `n_threads` when you want to leave CPU headroom for the rest of your game —
often a good idea on phones, where the big cores are also driving the game:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {"n_threads": 4})
```

Leave it unset to keep the detected default. It has little effect when the model is offloaded to
the GPU.

## Sharing a model between chats

There are scenarios where you would like to keep separate chat contexts (e.g. one for every NPC
in your game), but have only one model loaded. For this use case, load the model separately and
pass it to each chat:

```gdscript
var model = await NobodyWhoModel.create("./model.gguf", {})
var chat1 = await NobodyWhoChat.create(model, {})
var chat2 = await NobodyWhoChat.create(model, {})
```

NobodyWho takes care of the separation, such that your chat histories won't collide or interfere
with each other, while having only one model loaded.

## GPU

When loading a model you have the option to disable GPU acceleration. Pass `"use_gpu": false` in
the config when the chat loads the model from a path (when you pass a `NobodyWhoModel`, the GPU
setting is decided where that model was loaded):

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {"use_gpu": false})
var model = await NobodyWhoModel.create("./model.gguf", {"use_gpu": false})
```

By default `use_gpu` is `true`. So far, NobodyWho relies purely on
[Vulkan](https://www.vulkan.org); support of more architectures is planned (for details check out
our [issues](https://github.com/nobodywho-ooo/nobodywho/issues) or join us on
[Discord](https://discord.gg/qhaMc2qCYB)).

## Speculative decoding (MTP)

Some models come with **MTP** (Multi-Token Prediction) draft heads that let the target model
verify several candidate tokens per forward pass. See
[LLM Basics](/docs/llm-basics#speculative-decoding-mtp) for the underlying idea.

Load the model with a compatible draft-heads gguf (e.g. `mtp-gemma-4-E2B-it.gguf` for
Gemma-4-E2B) via `NobodyWhoModel.create`, and pass that model to the chat:

```gdscript
var model = await NobodyWhoModel.create("./gemma-4-e2b.gguf", {
    "draft_path": "./mtp-gemma-4-e2b.gguf",
})
var chat = await NobodyWhoChat.create(model, {})
```

Loading the draft heads adds around 5% to VRAM usage. Check how often the drafts are being
accepted with:

```gdscript
print("MTP acceptance rate: %s" % str(await chat.mtp_acceptance_rate()))
```

:::warning
Benchmark before enabling. MTP can hurt performance on Apple Silicon (Metal) and on high-entropy
workloads like creative prose.
:::

## Template variables

Chat templates are used internally by models to format conversation history into the expected
prompt format. Different models may support different template variables that control specific
behaviors. Template variables are boolean flags passed to the chat template that can enable or
disable certain features.

### Using template variables

You can set template variables when creating a chat or modify them on existing instances:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {
    "template_variables": {"enable_thinking": true},
})
```

You can also modify template variables on an existing chat:

```gdscript
# Set a single template variable
await chat.set_template_variable("enable_thinking", false)

# Set multiple template variables at once
await chat.set_template_variables({"enable_thinking": true, "verbose_mode": false})

# Get current template variables
var variables = await chat.get_template_variables()
print(variables) # {enable_thinking: true, verbose_mode: false}
```

With the next message sent, the updated settings will be propagated to the model.

### Example: Qwen3 and Qwen3.5 reasoning

The Qwen3 and Qwen3.5 model families support the `enable_thinking` template variable, which
controls whether the model should engage in explicit reasoning steps before answering:

```gdscript
var chat = await NobodyWhoChat.create("./model.gguf", {
    "template_variables": {"enable_thinking": true},
})
var stream = chat.ask("Solve this logic puzzle: ...")
```

When `enable_thinking` is enabled, these models will show their reasoning process before
providing the final answer.

### Model-specific variables

Different models may support different template variables depending on their chat template
implementation. The available variables and their effects depend entirely on how the model's chat
template is designed. Check your model's documentation to see which template variables are
supported.

:::info
Note that template variables are model-specific. If a model's chat template doesn't use a specific
variable, that variable will be ignored gracefully.
:::
