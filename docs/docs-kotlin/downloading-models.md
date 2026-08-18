---
title: Downloading models
description: How NobodyWho downloads, caches, and inspects GGUF models in Kotlin
sidebar_position: 6
---

NobodyWho can either load a model from a path on disk or download it for you on first use, caching it for subsequent runs. This page covers the available model path formats, how to access gated/private models, how to observe a download in progress, and how to inspect what's already in the local cache.

## Supported model path formats

The `modelPath` argument to `Model.load`, `Chat.fromPath`, and `Model.download` accepts:

| Form | Example | Notes |
| ---- | ------- | ----- |
| HuggingFace reference | `hf:owner/repo/file.gguf` | Downloaded and cached on first use |
| HTTPS URL | `https://example.com/model.gguf` | Downloaded and cached on first use |
| Local path | `./model.gguf` | Used as-is |

The HuggingFace prefix is case-insensitive and the `//` is optional — `hf:`, `hf://`, `huggingface:`, and `huggingface://` all mean the same thing. Remote models are downloaded to the platform cache directory on first load and re-used on subsequent runs.

## Android permissions

Android requires explicit approval, adding the internet permission to your app's `AndroidManifest.xml`:

```xml
<uses-permission android:name="android.permission.INTERNET" />
```

NobodyWho does not declare this permission for you, so apps that only load models from local paths keep working without requesting any network access.

## Downloading a gated model

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

## Inspecting the model cache

`getCachedModels()` returns every `.gguf` model in NobodyWho's cache directory, paired with its size in bytes. This is the same cache used by `Model.download` and by `Chat.fromPath`'s `hf://` paths.

```kotlin
import ai.nobodywho.getCachedModels

for (model in getCachedModels()) {
    println("${model.path} — ${model.size} bytes")
}
```
