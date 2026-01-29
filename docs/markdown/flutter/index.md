---
title: Getting started
description:  How to setup NobodyWho in Flutter
sidebar_title: Getting started
order: 0
---

## How do I get started?

First, install `nobodywho`.
```bash
flutter pub add nobodywho
```

Next, download a GGUF model you like - if you don't have a specific model in mind, try [this one](https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](../model-selection.md).

Once you have the `.gguf` file, make a `Chat` object and call `.ask()`!

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

await nobodywho.NobodyWho.init();

final chat = await nobodywho.Chat.fromPath(modelPath: './model.gguf');
final msg = await chat.ask('Is water wet?').completed();
print(msg); // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
