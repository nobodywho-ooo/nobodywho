---
title: Downloading models
description: How NobodyWho downloads, caches, and inspects GGUF models in Python
sidebar_position: 1
---

NobodyWho can either load a model from a path on disk or download it for you on first use, caching it for subsequent runs. This page covers the available model path formats, how to observe a download in progress, how to access gated/private models, and how to inspect what's already in the local cache.

## Supported model path formats

The `model_path` argument to `Chat`, `download_model`, and friends accepts:

| Form | Example | Notes |
| ---- | ------- | ----- |
| HuggingFace reference | `huggingface:owner/repo/file.gguf` or `hf://owner/repo/file.gguf` | Downloaded and cached on first use |
| HTTPS URL | `https://example.com/model.gguf` | Downloaded and cached on first use |
| Local path | `./model.gguf`, `/abs/path/to/model.gguf` | Used as-is |

Remote models are downloaded to the platform cache directory on first load and re-used on subsequent runs.

## Tracking download progress

When loading a remote model, pass an `on_download_progress` callback to observe the download. It receives `(downloaded_bytes, total_bytes)` and is not called for cached or local files. If you don't pass anything, NobodyWho prints a default terminal progress bar.

```python
from nobodywho import download_model

model_path = download_model(
    'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
    on_download_progress=lambda downloaded, total: print(f"{downloaded}/{total} bytes"),
)
```

## Downloading a gated model

Some HuggingFace models are private or gated by a license you need to accept. In both cases you need to be authorized to download the model weights.

You can manually download the GGUF file via your web browser and then point `Chat` at the local path:

```python
from nobodywho import Chat

chat = Chat('./model.gguf')
```

Or use `download_model` with an `Authorization` header:

```python
from nobodywho import Chat, download_model

model_path = download_model(
    'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
    headers={ "Authorization": "Bearer your_hf_token" }
)

chat = Chat(model_path)
```

You can generate a HuggingFace token in [your account settings](https://huggingface.co/settings/tokens).

## Inspecting the model cache

`get_cached_models` returns every `.gguf` model that lives in NobodyWho's cache directory, paired with its size in bytes. This is the same cache used by `download_model` and by `Chat`'s `huggingface:` paths.

```python
from nobodywho import get_cached_models

for path, size in get_cached_models():
    print(f"{path}: {size / 1024 / 1024:.1f} MiB")
```

- Paths are absolute.
- Sizes are in bytes.
- The list is empty if nothing has been downloaded yet.
- Raises `RuntimeError` if the cache directory cannot be read.
