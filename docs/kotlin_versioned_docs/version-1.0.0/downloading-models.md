---
title: Downloading Models
description: How to download and manage GGUF models
sidebar_position: 6
---

## Downloading gated models

Some HuggingFace models are either private or gated by a license that you need to accept. For both scenarios, you need to be authorized to download the model weights.

In that case, you can resort to manually accessing the model page through your web browser, getting the GGUF file downloaded and then pointing our chat instance to the path where you have stored it:

```kotlin
val chat = Chat.fromPath(modelPath = "./model.gguf")
```

Or you can use the `Model.download` function, where you can pass in the authorization token:

```kotlin
import ai.nobodywho.Model
import ai.nobodywho.Chat

val modelPath = Model.download(
    modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    headers = mapOf("Authorization" to "Bearer your_hf_token")
)

val chat = Chat.fromPath(modelPath = modelPath)
```

The token can be generated in [your account settings](https://huggingface.co/settings/tokens).

## Tracking download progress

When loading a remote model, pass an `onDownloadProgress` callback to observe the download. It receives `(downloadedBytes, totalBytes)` and is not called for cached or local files.

```kotlin
val model = Model.load(
    modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
) { downloaded, total ->
    println("$downloaded / $total bytes")
}
```
