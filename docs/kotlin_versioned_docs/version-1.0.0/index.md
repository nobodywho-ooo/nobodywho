---
title: Getting started
description: How to setup NobodyWho in Kotlin
sidebar_position: 0
---

## How do I get started?

First, add NobodyWho to your `build.gradle.kts`:

```kotlin
// Android
implementation("ai.nobodywho:nobodywho-android:1.0.0")

// Desktop JVM (Linux, macOS, Windows)
implementation("ai.nobodywho:nobodywho:1.0.0")
```

Next, pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass an `hf://` path. See [model selection](/docs/model-selection) for recommendations.

Then create a `Chat` and call `.ask`!

```kotlin
import ai.nobodywho.Chat
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    val chat = Chat.fromPath(
        modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
    )

    // stream tokens as they are generated
    chat.ask("Is water wet?").asFlow().collect { token ->
        print(token)
    }

    // ...or get the entire response as a single string
    val response = chat.ask("Is water wet?").completed()
    println(response)
}
```

On Android, use `lifecycleScope` or `viewModelScope` instead of `runBlocking`.

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
