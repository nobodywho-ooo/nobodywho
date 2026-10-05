---
title: Downloading models
description: How NobodyWho downloads, caches, and inspects GGUF models in C#
sidebar_position: 6
---

NobodyWho can either load a model from a path on disk or download it for you on first use, caching it for subsequent runs. This page covers the available model path formats, how to access gated/private models, how to observe a download in progress, and how to inspect what's already in the local cache.

## Supported model path formats

The `modelPath` argument to `Model.LoadAsync`, `Chat.FromPathAsync`, and `Model.DownloadAsync` accepts:

| Form | Example | Notes |
| ---- | ------- | ----- |
| HuggingFace reference | `hf:owner/repo/file.gguf` | Downloaded and cached on first use |
| llama.cpp-style reference | `owner/repo:quantization` | Downloaded and cached on first use |
| HTTPS URL | `https://example.com/model.gguf` | Downloaded and cached on first use |
| Local path | `./model.gguf` | Used as-is |

The HuggingFace prefix is case-insensitive and the `//` is optional — `hf:`, `hf://`, `huggingface:`, and `huggingface://` all mean the same thing. Remote models are downloaded to the platform cache directory on first load and re-used on subsequent runs.

The llama.cpp-style reference takes no prefix, and names a HuggingFace repo whose name must end in `-GGUF` — the convention llama.cpp relies on to work out the filename. NobodyWho resolves it the same way, so `ggml-org/gemma-3-1b-it-GGUF:Q8_0` fetches `gemma-3-1b-it-Q8_0.gguf` from the `ggml-org/gemma-3-1b-it-GGUF` repo. Both the `-GGUF` suffix and the quantization are required; without them the string is read as a local path rather than a download. **Note:** llama.cpp will take the first model in the repo if there is no exact match for the quantization, but NobodyWho will fail with an error if the quantization is not found.

## Downloading a gated model

Some HuggingFace models are either private or gated by a license that you need to accept. For both scenarios, you need to be authorized to download the model weights.

In that case, you can resort to manually accessing the model page through your web browser, getting the GGUF file downloaded and then pointing our chat instance to the path where you have stored it:

```csharp
using var chat = await Chat.FromPathAsync("./model.gguf");
```

Or you can use `Model.DownloadAsync`, where you can pass in the authorization token. It returns the local path of the downloaded file:

```csharp
using NobodyWho;

string modelPath = await Model.DownloadAsync(
    "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    headers: new Dictionary<string, string> { ["Authorization"] = "Bearer your_hf_token" });

using var chat = await Chat.FromPathAsync(modelPath);
```

The token can be generated in [your account settings](https://huggingface.co/settings/tokens). Avoid hard-coding it — read it from an environment variable or your app's secret store instead.

## Tracking download progress

When loading a remote model, pass an `IProgress<DownloadProgress>` as `progress` to observe the download. It receives the bytes downloaded so far and the total, and is not called for cached or local files.

```csharp
var progress = new Progress<DownloadProgress>(p =>
    Console.WriteLine($"{p.Downloaded} / {p.Total} bytes"));

using var model = await Model.LoadAsync(
    "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    progress: progress);
```

`Chat.FromPathAsync`, `Encoder.FromPathAsync`, `CrossEncoder.FromPathAsync` and `Model.DownloadAsync` take the same `progress` parameter. `Progress<T>` reports on the synchronization context it was created on, so in a UI app you can update a progress bar directly from the callback.

## Cancelling a download

Pass a `CancellationToken` to stop waiting for a download or load, for example when the user navigates away:

```csharp
using var cts = new CancellationTokenSource(TimeSpan.FromMinutes(5));

try
{
    using var model = await Model.LoadAsync(
        "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
        cancellationToken: cts.Token);
}
catch (OperationCanceledException)
{
    Console.WriteLine("Gave up waiting for the model.");
}
```

Cancelling only stops the wait: the download itself keeps running in the background and finishes into the cache, so the next load is fast. A model that finishes loading after you cancelled is freed automatically.

## Inspecting the model cache

`Model.GetCachedModels()` returns every `.gguf` model in NobodyWho's cache directory, with its size in bytes. This is the same cache used by `Model.DownloadAsync` and by `hf://` paths passed to `Model.LoadAsync` or `Chat.FromPathAsync`.

```csharp
foreach (CachedModel cached in Model.GetCachedModels())
{
    Console.WriteLine($"{cached.Path} — {cached.Size} bytes");
}
```
