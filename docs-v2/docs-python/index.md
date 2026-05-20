---
title: Getting started
description:  How to setup NobodyWho in Python
sidebar_position: 0
---



## How do I get started?

First, install `nobodywho`.
```bash
pip install nobodywho
```
Or preferably:
```bash
uv add nobodywho
```

Next, pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass a `huggingface:` path. See [model selection](/docs/model-selection) for recommendations.

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

## Downloading gated model

Some huggingface models are either private or gated by a license that you need to accept.
For both scenarios, you need to be authorized to download the model weights.

In that case, you can resort to manually accessing the model page through your web browser,
getting the GGUF file downloaded and then pointing our chat instance to the path where you have stored it:
```python
from nobodywho import Chat

chat = Chat('./model.gguf')
```

Or you can use the `download_model` function, where you can pass in the authorization token:
```python
from nobodywho import Chat, download_model

model_path = download_model(
    'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
    headers={ "Authorization": "Bearer your_hf_token" }
)

chat = Chat(model_path)
```

The token can be then generated in [your account settings](https://huggingface.co/settings/tokens).

## Tracking download progress

When loading a remote model, pass an `on_download_progress` callback to observe the download. It receives `(downloaded_bytes, total_bytes)` and is not called for cached or local files. If you don't pass anything, NobodyWho prints a default terminal progress bar.

```python
from nobodywho import download_model

model_path = download_model(
    'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
    on_download_progress=lambda downloaded, total: print(f"{downloaded}/{total} bytes"),
)
```

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
