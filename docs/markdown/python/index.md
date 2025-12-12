---
title: Getting started
description:  NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Getting started
order: 0
---



## How do I get started?

First, install `nobodywho`.
```bash
pip install nobodywho
```

Next, download a GGUF model you like - if you don't have a specific model in mind, try [this one](https://huggingface.co/Qwen/Qwen3-4B-GGUF/resolve/main/Qwen3-4B-Q5_0.gguf?download=true). If you are interested, click [here](../model-selection.md) to read more about model selection.

Once you have the `.gguf` file, make a `Chat` object and call `.ask()`!

```python
from nobodywho import Chat

chat = Chat('./model.gguf')
response = chat.ask('Hello world?').completed()
print(response) # Hello world!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.

## What to explore next?

<div style="background-color: red;">
    TODO
</div>
