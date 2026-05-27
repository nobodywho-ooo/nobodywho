---
title: Getting started
description: How to setup NobodyWho in Kotlin
sidebar_position: 0
---

## How do I get started?

Add NobodyWho to your project:

```kotlin
// Android (build.gradle.kts)
implementation("ai.nobodywho:nobodywho-android:0.1.0")

// Desktop JVM — Linux, macOS, Windows (build.gradle.kts)
implementation("ai.nobodywho:nobodywho:0.1.0")
```

Both artifacts use the same API — the only difference is the bundled native libraries.

Now you are ready to pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass an `hf://` path. See [model selection](/docs/model-selection) for recommendations.

Then create a `Chat` and call `.ask`!

```kotlin
import ai.nobodywho.Chat

val chat = Chat.fromPath(
    modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
)

val response = chat.ask("Is water wet?").completed()
println(response) // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

## Downloading gated models

Some HuggingFace models are either private or gated by a license that you need to accept. You can use the `Model.download` function with custom headers:

```kotlin
import ai.nobodywho.Model
import ai.nobodywho.Chat

val modelPath = Model.download(
    modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    headers = mapOf("Authorization" to "Bearer your_hf_token")
)

val chat = Chat(model = Model.load(modelPath))
```

The token can be generated in [your account settings](https://huggingface.co/settings/tokens).

## Tracking download progress

When loading a remote model, pass an `onDownloadProgress` callback to observe the download:

```kotlin
val model = Model.load(
    modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
) { downloaded, total ->
    println("$downloaded / $total bytes")
}
```

## Coroutines

All NobodyWho operations that involve model loading or inference are `suspend` functions. Use them inside a coroutine scope:

```kotlin
import kotlinx.coroutines.runBlocking
import ai.nobodywho.Chat

fun main() = runBlocking {
    val chat = Chat.fromPath(modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf")
    val response = chat.ask("Hello!").completed()
    println(response)
}
```

In Android, use `lifecycleScope` or `viewModelScope` instead of `runBlocking`.

## Minimum recommended specs

- Android: Snapdragon 855 / Adreno 640 / 6 GB RAM or better.
- Desktop: Any modern x86_64 or ARM64 system with at least 4 GB RAM. GPU acceleration via Vulkan on Linux/Windows, Metal on macOS.

## Feedback & Contributions

We welcome your feedback and ideas!

- Bug Reports & Improvements: If you encounter a bug or have suggestions, please open an issue on our [Issues](https://github.com/nobodywho-ooo/nobodywho/issues) page.
- Feature Requests & Questions: For new feature requests or general questions, join the discussion on our [Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) page.
