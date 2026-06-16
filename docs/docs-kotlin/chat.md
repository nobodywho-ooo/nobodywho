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
    Message.User(content = "What is water?")
))
```

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

The default is `4096`. When the context fills up during a conversation, NobodyWho automatically shrinks it by removing old messages (keeping the system prompt and first user message).

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

## GPU

When loading a model, GPU acceleration is enabled by default:

```kotlin
val model = Model.load(modelPath = "./model.gguf", useGpu = true)
```

NobodyWho uses Vulkan on Linux/Windows and Metal on macOS for GPU acceleration.

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