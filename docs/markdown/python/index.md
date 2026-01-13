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

Next, download a GGUF model you like - if you don't have a specific model in mind, try [this one](https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](../model-selection.md).

Once you have the `.gguf` file, make a `Chat` object and call `.ask()`!

```python
from nobodywho import Chat

chat = Chat('./model.gguf')
response = chat.ask('Is water wet?')

# print each token as it is generated
for token in response:
    print(token, end="", flush=True)

# ...or get the entire response as a single string
full_response = response.completed()
print(full_response)
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
