---
title: Getting started
description:  How to setup NobodyWho in Python
sidebar_title: Getting started
order: 0
---



## How do I get started?

First, install `nobodywho`.
```bash
pip install nobodywho
```

Next, pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass a `huggingface:` path. See [model selection](../model-selection.md) for recommendations.

Then make a `Chat` object and call `.ask()`!

```python
from nobodywho import Chat

chat = Chat('huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf')
response = chat.ask('Is water wet?')

# print each token as it is generated
for token in response:
    print(token, end="", flush=True)

# ...or get the entire response as a single string
full_response = response.completed()
print(full_response)
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

## Tracking download progress

When loading a remote model, pass an `on_download_progress` callback to observe the download. It receives `(downloaded_bytes, total_bytes)` and is not called for cached or local files. If you don't pass anything, NobodyWho prints a default terminal progress bar.

```python
from nobodywho import Model

model = Model(
    'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
    on_download_progress=lambda d, t: print(f"{d}/{t} bytes"),
)
```

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
