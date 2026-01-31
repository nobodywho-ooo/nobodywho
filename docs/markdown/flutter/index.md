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

Next you need to import NobodyWho and we highly suggets you do this using the namespace `nobodywho` like so:
```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;
```
since we have generic names such as `Model` and `Chat` in our package. 
After you have imported the package it is very important that the next step is done correctly. As we dynamically link the rust binaries you must make 
the following function call exactly once in your application!

```dart
await nobodywho.NobodyWho.init();
```

A call to any of the functions in NobodyWho will result in an error before `.init()` has been called. 
However a second call to `.init()` will also result in an error, so you should be mindful about when you make this call.
We suggest you make it as early and as close to the root of your app as possible, as even though it is async it is a very fast operation.

With that setup done we can move on to the exiting stuff! We will in the rest of the docs that 
you have imported NobodyWho using namespacing and that `.init()` has been called. 

Now you are ready to download a GGUF model you like - if you don't have a specific model in mind, try [this one](https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](../model-selection.md).

Once you have the `.gguf` file, the next step is to create a `Chat` object and call `.ask`!


``` dart

final chat = await nobodywho.Chat.fromPath(modelPath: './model.gguf');
final msg = await chat.ask('Is water wet?').completed();
print(msg); // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
