---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_position: 1
---

Every interaction with your LLM starts with a `Chat` object.

## Creating a Chat

The simplest way is using `Chat.fromPath`:

```kotlin
val chat = Chat.fromPath(modelPath = "./model.gguf")
```

This is a suspend function since loading a model can take a bit of time, but it won't block your UI thread.

Another way is to load the model separately and share it between multiple chats:

```kotlin
val model = Model.load(modelPath = "./model.gguf")
val chat1 = Chat(model = model)
val chat2 = Chat(model = model)
```

NobodyWho takes care of the separation, so your chat histories won't collide or interfere with each other.

You can pass `"auto"` as `modelPath` to select a chat model based on available memory.

## Prompts and responses

The `Chat.ask()` function sends your message to the LLM, which then starts generating a response:

```kotlin
val chat = Chat.fromPath(modelPath = "./model.gguf")
val stream = chat.ask("Is water wet?")
```

The return type is a `TokenStream`. If you want to read the response as it generates, collect it as a Flow:

```kotlin
chat.ask("Is water wet?").asFlow().collect { token ->
    print(token)
}
```

If you just want the complete response, call `completed()`:

```kotlin
val fullResponse = chat.ask("Is water wet?").completed()
```

All messages and responses are stored in the `Chat`, so the next `ask()` remembers the conversation.

## Chat history

Inspect the messages inside the `Chat`:

```kotlin
val msgs = chat.getChatHistory()
println(msgs[0].content) // "Is water wet?"
```

Or set the history directly:

```kotlin
chat.setChatHistory(listOf(
    Message.User("What is water?")
))
```

## Chat completion

If you would rather pass the whole conversation on every call than let the `Chat` remember it, use `complete()`:

```kotlin
val response = chat.complete(listOf(
    Message.System("You are a helpful assistant."),
    Message.User("Who was the first person to walk on the moon?"),
    Message.Assistant("Neil Armstrong."),
    Message.User("Which year did he do it?")
)).completed()
```

You get back the same `TokenStream` as from `ask()`.

The list you pass **becomes** the chat history, replacing whatever was there, and the response is added to it — so `ask()` continues that same conversation. A system message at the front sets the chat's system prompt; leave it out and the prompt already on the chat is kept.

A system message further in stays in the history, for the chat template to render in place. Templates without a system role throw instead, since only a leading one can be folded into the first user message.

The list must not be empty and must end in a user or tool message. Anything else throws.

### Per-turn settings

`complete()` takes an optional `Options` carrying the chat's other settings. It follows the same rule as the system message: what it sets stays set, what it leaves out is kept.

```kotlin
chat.complete(
    listOf(Message.User("Name one fruit.")),
    Options(
        sampler = SamplerPresets.greedy(),
        templateVariables = mapOf("enable_thinking" to false),
    ),
).completed()

// Both are now the chat's settings, so the next call need not repeat them
chat.complete(listOf(Message.User("Name another."))).completed()
```

Fill in all three fields and the call no longer depends on what the chat is currently holding — useful if you drive it entirely through `complete()`.

Changing `tools` re-selects the chat template and rewrites the system-prompt region, so that turn re-prefills from near token zero. Set it when it changes, not on every call.

## System prompt

A system prompt guides the model's overall behavior. Some models ship with a built-in default.

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    systemPrompt = "You are a mischievous assistant!"
)
```

The system prompt persists until the chat context is reset.

## Context

The context is the token window the LLM currently considers. Larger context means more computational overhead:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    contextSize = 4096u
)
```

The default is `4096`. When the context fills up during a conversation, NobodyWho automatically shrinks it by removing old messages (keeping the system prompt and first user message). You can check the maximum context size the model was trained with using `model.maxCtx` — setting `contextSize` above this value has no benefit.

To reset the context with a new system prompt and tools:

```kotlin
chat.resetContext(systemPrompt = "New system prompt", tools = listOf())
```

To just clear the history without changing settings:

```kotlin
chat.resetHistory()
```

To inspect how much of the context is currently in use, call `getStats()`:

```kotlin continuation
val stats = chat.getStats()
println("Using ${stats.contextUsed} of ${stats.contextSize} tokens")
```

## CPU threads

When layers run on the CPU, NobodyWho picks a thread count for you: one per *performance* core,
not one per logical CPU. Efficiency cores end up pacing the whole thread pool, so using every
CPU is usually slower — see [LLM Basics](/docs/llm-basics#cpu-threads) for the numbers.

Override it with `threadCount` when you want to leave CPU headroom for the rest of your app —
often a good idea on phones, where the big cores are also driving the UI:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    threadCount = 4u
)
```

Leave it unset — or pass `null` — to keep the detected default. Values above the device's CPU
count are clamped, and it has little effect when the model is offloaded to the GPU.

## GPU

When loading a model, GPU acceleration is enabled by default:

```kotlin
val model = Model.load(modelPath = "./model.gguf", useGpu = true)
```

NobodyWho uses Vulkan on Linux/Windows and Metal on macOS for GPU acceleration.

## Speculative decoding (MTP)

Some models come with **MTP** (Multi-Token Prediction) draft heads that let the target model verify several candidate tokens per forward pass. See [LLM Basics](/docs/llm-basics#speculative-decoding-mtp) for the underlying idea.

Load the model with a compatible draft-heads gguf (e.g. `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B) and pass an `MtpConfig` when constructing the chat:

```kotlin
val model = Model.load(
    modelPath = "./gemma-4-e2b.gguf",
    draftModelPath = "./mtp-gemma-4-e2b.gguf"
)
// MtpConfig() uses the default drafter tuning; tune with MtpConfig(kMax = ..., pMin = ...)
val chat = Chat(model = model, mtp = MtpConfig())
```

`Chat.fromPath` accepts the same two parameters if you don't need to share the model:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./gemma-4-e2b.gguf",
    draftModelPath = "./mtp-gemma-4-e2b.gguf",
    mtp = MtpConfig()
)
```

Loading the draft heads adds around 5% to VRAM usage.

:::warning
Benchmark before enabling. MTP can hurt performance on Apple Silicon (Metal) and on high-entropy workloads like creative prose.
:::

## Template Variables

Chat templates are used internally by models to format conversation history. Template variables are boolean flags that control specific behaviors.

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    templateVariables = mapOf("enable_thinking" to true)
)
```

You can also modify them on an existing chat:

```kotlin
chat.setTemplateVariable("enable_thinking", false)
val variables = chat.getTemplateVariables()
```

### Example: Qwen3 Reasoning

The Qwen3 model family supports `enable_thinking`, which controls whether the model shows its reasoning process before answering:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    templateVariables = mapOf("enable_thinking" to true)
)
val response = chat.ask("Solve this logic puzzle: ...").completed()
```

:::info
Template variables are model-specific. If a model's chat template doesn't use a specific variable, it will be ignored.

:::