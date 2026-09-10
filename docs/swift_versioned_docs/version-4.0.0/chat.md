---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_position: 1
---

As you may have noticed in the [welcome guide](./), every interaction with your LLM starts by creating a `Chat` object.
In the following sections, we talk about which configuration options it has, and when to use them.

## Creating a Chat

There are two main ways of creating a `Chat` object and the difference lies in when the model file is loaded.
The simplest way is using `Chat.fromPath`:

```swift
import NobodyWho

let chat = try await Chat.fromPath(modelPath: "/path/to/model.gguf")
```

The `modelPath` parameter accepts a local file path, a Hugging Face `hf://` URL, an `https://` URL, or `"auto"` to select a chat model based on available memory:

```swift
// From a Hugging Face repository
let chat = try await Chat.fromPath(
    modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
)

// From an HTTPS URL
let chat = try await Chat.fromPath(
    modelPath: "https://example.com/model.gguf"
)
```

When loading from a remote URL, you can track download progress with the `onDownloadProgress` callback:

```swift
let chat = try await Chat.fromPath(
    modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
) { downloaded, total in
    print("Downloaded \(downloaded)/\(total) bytes")
}
```

This function is async since loading a model can take a bit of time, but this should not block any of your UI.
Another way to achieve the same thing is to load the model separately and then use the `Chat` constructor:

```swift
import NobodyWho

let model = try await Model.load(modelPath: "/path/to/model.gguf")
let chat = try Chat(model: model)
```

The `Model.load` function also supports `hf://` and `https://` URLs, as well as `onDownloadProgress`:

```swift
let model = try await Model.load(
    modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
) { downloaded, total in
    print("Downloaded \(downloaded)/\(total) bytes")
}
```

This allows for sharing the model between several `Chat` instances.

## Prompts and responses

The `chat.ask()` function is central to NobodyWho. This function sends your message to the LLM, which then starts generating a response.

```swift
let chat = try await Chat.fromPath(modelPath: "/path/to/model.gguf")
let response = chat.ask("Is water wet?")
```

The return type of `ask` is a `TokenStream`, which conforms to `AsyncSequence`.
If you want to start reading the response as soon as possible, you can iterate over it using `for await`.
Each token is either an individual word or a fragment of a word.

```swift
for await token in response {
    print(token, terminator: "")
}
```

If you just want to get the complete response, you can call `completed()`.
This will return the entire response string once the model is done generating.

```swift
let fullResponse = try await response.completed()
```

All of your messages and the model's responses are stored in the `Chat` object, so the next time you call `chat.ask()`, it will remember the previous messages.

## Stopping generation

If you need to cancel the model's response while it is still generating (for example, when the user taps a "Stop" button), call `stopGeneration()`:

```swift
chat.stopGeneration()
```

This immediately stops token generation. Any tokens already produced are still available in the stream. The partial response is added to the chat history, so the conversation remains coherent. Note that `stopGeneration()` is a synchronous call and can be invoked from any thread.

## Chat history

If you want to inspect the messages inside the `Chat` object, you can use `getChatHistory`.

```swift
let msgs = try await chat.getChatHistory()
print(msgs[0]) // The first message
```

Similarly, if you want to edit what messages are in the context, you can use `setChatHistory`:

```swift
try await chat.setChatHistory([
    .user("What is water?")
])
```

## Chat completion

If you would rather pass the whole conversation on every call than let the `Chat` remember it, use `complete()`:

```swift
let response = try await chat.complete([
    .system("You are a helpful assistant."),
    .user("Who was the first person to walk on the moon?"),
    .assistant("Neil Armstrong."),
    .user("Which year did he do it?"),
]).completed()
```

You get back the same `TokenStream` as from `ask()`, so you can also `for try await token in ...` over it.

The list you pass **becomes** the chat history, replacing whatever was there, and the response is added to it — so `ask()` continues that same conversation. A system message at the front sets the chat's system prompt; leave it out and the prompt already on the chat is kept.

The list must not be empty, must end in a user or tool message, and may only have a system message first. Anything else throws.

### Per-turn settings

`complete()` takes an optional `Options` carrying the chat's other settings. It follows the same rule as the system message: what it sets stays set, what it leaves out is kept.

```swift
_ = try await chat.complete(
    [.user("Name one fruit.")],
    options: Options(
        sampler: SamplerPresets.greedy(),
        templateVariables: ["enable_thinking": false]
    )
).completed()

// Both are now the chat's settings, so the next call need not repeat them
_ = try await chat.complete([.user("Name another.")]).completed()
```

Fill in all three fields and the call no longer depends on what the chat is currently holding — useful if you drive it entirely through `complete()`.

Changing `tools` re-selects the chat template and rewrites the system-prompt region, so that turn re-prefills from near token zero. Set it when it changes, not on every call.

## System prompt

A system prompt is a special message put into the chat context, which should guide its overall behavior.
Some models ship with a built-in system prompt. If you don't specify a system prompt yourself, NobodyWho will fall back to using the model's default system prompt.

You can specify a system prompt when creating a `Chat`:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    systemPrompt: "You are a mischievous assistant!"
)
```

This `systemPrompt` is then persisted until the chat context is reset.

## Context

The context is the text window which the LLM currently considers. Specifically this is the number of tokens the LLM keeps in memory for your current conversation.
A bigger context size means more computational overhead, so it makes sense to constrain it. This can be done with the `contextSize` setting at creation time:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    contextSize: 4096
)
```

The default value is `4096`, however this is mainly useful for short and simple conversations. Choosing the right context size is quite important and depends heavily on your use case. You can check the maximum context size the model was trained with using `model.maxCtx` — setting `contextSize` above this value has no benefit.

Even with a properly selected context size it might happen that you fill up your entire context during a conversation. When this happens, NobodyWho will shrink the context for you. Currently this is done by removing old messages (apart from the system prompt and the first user message) from the chat history, until the size reaches `contextSize / 2`. The KV cache is also updated automatically.

To reset the current context content, call `resetContext()` with a new system prompt and potentially changed tools.

```swift
try await chat.resetContext(systemPrompt: "New system prompt", tools: [])
```

If you don't want to change the already set defaults (`systemPrompt`, `tools`), but only reset the context, then go for `resetHistory`.

To inspect how much of the context is currently in use, call `getStats()`:

```swift continuation
let stats = try await chat.getStats()
print("Using \(stats.contextUsed) of \(stats.contextSize) tokens")
```

## CPU threads

When layers run on the CPU, NobodyWho picks a thread count for you: one per *performance* core,
not one per logical CPU. Efficiency cores end up pacing the whole thread pool, so using every
CPU is usually slower — see [LLM Basics](/docs/llm-basics#cpu-threads) for the numbers.

Override it with `threadCount` when you want to leave CPU headroom for the rest of your app:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    threadCount: 4
)
```

Leave it unset — or pass `nil` — to keep the detected default. Values above the device's CPU
count are clamped, and it has little effect when the model is offloaded to the GPU.

## Sharing model between contexts

There are scenarios where you would like to keep separate chat contexts (e.g. for every user of your app), but have only one model loaded. In this case you must load the model separately from creating the `Chat` instance.

```swift
let model = try await Model.load(modelPath: "/path/to/model.gguf")
let chat1 = try Chat(model: model)
let chat2 = try Chat(model: model)
```

NobodyWho will then take care of the separation, such that your chat histories won't collide or interfere with each other, while having only one model loaded.

## GPU

When using `Model.load` or `Chat.fromPath` you have the option to disable GPU acceleration:

```swift
let model = try await Model.load(modelPath: "/path/to/model.gguf", useGpu: false)
```

By default `useGpu` is set to `true`, which uses Metal on Apple platforms.

## Speculative decoding (MTP)

Some models come with **MTP** (Multi-Token Prediction) draft heads that let the target model verify several candidate tokens per forward pass. See [LLM Basics](/docs/llm-basics#speculative-decoding-mtp) for the underlying idea.

Load the model with a compatible draft-heads gguf (e.g. `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B) and pass an `MtpConfig` when constructing the chat:

```swift
let model = try await Model.load(
    modelPath: "/path/to/gemma-4-e2b.gguf",
    draftModelPath: "/path/to/mtp-gemma-4-e2b.gguf"
)
// MtpConfig() uses the default drafter tuning; tune with MtpConfig(kMax: ..., pMin: ...)
let chat = try Chat(model: model, mtp: MtpConfig())
```

`Chat.fromPath` accepts the same two parameters if you don't need to share the model:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/gemma-4-e2b.gguf",
    draftModelPath: "/path/to/mtp-gemma-4-e2b.gguf",
    mtp: MtpConfig()
)
```

Loading the draft heads adds around 5% to VRAM usage.

:::warning
Benchmark before enabling. MTP can hurt performance on Apple Silicon (Metal) and on high-entropy workloads like creative prose.
:::

## Template Variables

Chat templates are used internally by models to format conversation history into the expected prompt format. Different models may support different template variables that control specific behaviors. Template variables are boolean flags passed to the chat template that can enable or disable certain features.

### Using Template Variables

You can set template variables when creating a chat or modify them on existing instances:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    templateVariables: ["enable_thinking": true]
)
```

You can also modify template variables on an existing chat instance:

```swift
try await chat.setTemplateVariable(name: "enable_thinking", value: true)
let variables = try await chat.getTemplateVariables()
```

### Example: Qwen3 and Qwen3.5 Reasoning

The Qwen3 and Qwen3.5 model families support the `enable_thinking` template variable, which controls whether the model should engage in explicit reasoning steps before answering:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    templateVariables: ["enable_thinking": true]
)
let response = chat.ask("Solve this logic puzzle: ...")
```

When `enable_thinking` is enabled, these models will show their reasoning process before providing the final answer.

:::info
Note that template variables are model-specific. If a model's chat template doesn't use a specific variable, that variable will be ignored gracefully.

:::