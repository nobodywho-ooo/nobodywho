---
title: Welcome to NobodyWho!
description:  NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Welcome
order: 0
---

## What is NobodyWho?

NobodyWho is a lightweight, open-source AI engine for local LLM inference. <br/>
Simple, privacy-oriented with no infrastructure needed.

In short, if you want to run a LLM, and integrate it with [tools](./tool-calling.md), configure its output,
enable real-time streaming of tokens, or maybe use it for creation of embeddings, NobodyWho can help.

All of this is enabled by [Llama.cpp](https://github.com/ggml-org/llama.cpp), while having nice, simple Python API. All of this is only a single `pip install` away. No additional docker container needed.

## How do I get started?

First, install `nobodywho`.
```bash
pip install nobodywho
```

Next, download a model you like - we can help with this.
When you have the `.gguf` file, just ask!
```python
from nobodywho import Chat

chat = Chat('./model.gguf')
response = chat.ask('Hello world?').completed()
print(response) # Hello world!
```

This is a super simple example, but we believe,
that examples which do simple things, should be simple!

However, you can follow to plenty of more advanced stuff.

## What to explore next?

<div style="background-color: red;">
    TODO
</div>
