---
title: Welcome to NobodyWho!
description:  NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Welcome
order: 0
---

## What is NobodyWho?

NobodyWho is a lightweight, open-source AI engine for local LLM inference. <br/>
We provide a simple, privacy forward way of interacting with LLMs with no infrastructure needed!

In short, if you want to run a LLM, and integrate it with [tools](./tool-calling.md), configure its output,
enable real-time streaming of tokens, or maybe use it for creation of embeddings, NobodyWho makes it easy.

All of this is enabled by [Llama.cpp](https://github.com/ggml-org/llama.cpp), while having nice, simple Python API.

No need to mess around with docker containers, GPU servers, API keys, etc. Just pip install and get going.

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
