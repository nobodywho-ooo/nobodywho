---
title: Downloading models
description: How NobodyWho downloads, caches, and inspects GGUF models in React Native
sidebar_position: 1
---

NobodyWho can either load a model from a path on disk or download it for you on first use, caching it for subsequent runs. This page covers the available model path formats, how to observe a download in progress, how to access gated/private models, and how to inspect what's already in the local cache.

## Supported model path formats

The `modelPath` option to `Chat.fromPath` and `downloadModel` accepts:

| Form | Example | Notes |
| ---- | ------- | ----- |
| HuggingFace reference | `huggingface:owner/repo/file.gguf` or `hf://owner/repo/file.gguf` | Downloaded and cached on first use |
| HTTPS URL | `https://example.com/model.gguf` | Downloaded and cached on first use |
| Local path | `./model.gguf`, `/abs/path/to/model.gguf` | Used as-is |

Remote models are downloaded to the platform cache directory on first load and re-used on subsequent runs.

## Tracking download progress

When loading a remote model, pass an `onDownloadProgress` option to observe the download. It receives `(downloaded, total)` byte counts, is throttled to roughly 10 Hz with a guaranteed final emit on completion, and is not called for cached or local files.

```typescript
const chat = await Chat.fromPath({
  modelPath: "huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
  onDownloadProgress: (downloaded, total) => {
    console.log(`${downloaded} / ${total} bytes`);
  },
});
```

## Downloading a gated model

Some HuggingFace models are private or gated by a license you need to accept. In both cases you need to be authorized to download the model weights.

You can manually download the GGUF file via your web browser, place it on the device, and then point `Chat.fromPath` at the local path:

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({ modelPath: "./model.gguf" });
```

Or use `downloadModel` with an `Authorization` header:

```typescript
import { downloadModel, Chat } from "react-native-nobodywho";

const modelPath = await downloadModel({
  modelPath: "huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
  headers: { Authorization: "Bearer your_hf_token" },
});

const chat = await Chat.fromPath({ modelPath });
```

You can generate a HuggingFace token in [your account settings](https://huggingface.co/settings/tokens).

## Inspecting the model cache

`getCachedModels()` returns every `.gguf` model in NobodyWho's cache directory, paired with its size in bytes. This is the same cache used by `downloadModel` and by `Chat.fromPath`'s `huggingface:` paths.

```typescript
import { getCachedModels } from "react-native-nobodywho";

for (const model of getCachedModels()) {
  console.log(`${model.path}: ${Number(model.size)} bytes`);
}
```

Each entry has:

- `path: string` — absolute path to the cached `.gguf` file
- `size: bigint` — size in bytes (the underlying Rust `u64`, exposed as JavaScript `bigint`)

The array is empty if nothing has been downloaded yet. The call throws if the cache directory cannot be read.
