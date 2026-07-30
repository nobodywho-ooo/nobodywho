---
title: Downloading models
description: How NobodyWho downloads, caches, and inspects GGUF models in Flutter
sidebar_position: 1
---

NobodyWho can either load a model from a path on disk or download it for you on first use, caching it for subsequent runs. This page covers the available model path formats, how to observe a download in progress, how to access gated/private models, and how to inspect what's already in the local cache.

## Supported model path formats

The `modelPath` argument to `Chat.fromPath`, `downloadModel`, and friends accepts:

| Form | Example | Notes |
| ---- | ------- | ----- |
| HuggingFace reference | `hf:owner/repo/file.gguf` | Downloaded and cached on first use |
| HTTPS URL | `https://example.com/model.gguf` | Downloaded and cached on first use |
| Local path | `./model.gguf` | Used as-is |

The HuggingFace prefix is case-insensitive and the `//` is optional — `hf:`, `hf://`, `huggingface:`, and `huggingface://` all mean the same thing. Remote models are downloaded to the platform cache directory on first load and re-used on subsequent runs.

## Tracking download progress

When loading a remote model, pass an `onDownloadProgress` callback to observe the download. It receives `(downloadedBytes, totalBytes)`, is throttled to roughly 10 Hz with a guaranteed final emit on completion, and is not called for cached or local files.

```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: 'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
  onDownloadProgress: (downloaded, total) {
    print('$downloaded / $total bytes');
  },
);
```

## Downloading a gated model

Some HuggingFace models are private or gated by a license you need to accept. In both cases you need to be authorized to download the model weights.

You can manually download the GGUF file via your web browser and then point `Chat.fromPath` at the local path:

```dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
);
```

Or use `downloadModel` with an `Authorization` header:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final modelPath = await nobodywho.downloadModel(
  modelPath: 'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
  headers: {'Authorization': 'Bearer your_hf_token'},
);

final chat = await nobodywho.Chat.fromPath(modelPath: modelPath);
```

You can generate a HuggingFace token in [your account settings](https://huggingface.co/settings/tokens).

## Inspecting the model cache

`getCachedModels` returns every `.gguf` model that lives in NobodyWho's cache directory, paired with its size in bytes. This is the same cache used by `downloadModel` and by `Chat.fromPath`'s `huggingface:` paths. The call is synchronous.

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final models = nobodywho.getCachedModels();
for (final (path, size) in models) {
  print('$path: ${size ~/ BigInt.from(1024 * 1024)} MiB');
}
```

- Paths are absolute.
- Sizes are `BigInt` byte counts (the underlying Rust `usize`).
- The list is empty if nothing has been downloaded yet.
- Throws if the cache directory cannot be read.
