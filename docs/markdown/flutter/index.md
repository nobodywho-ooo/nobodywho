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

Now you are ready to pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass a `huggingface:` path. See [model selection](../model-selection.md) for recommendations.

Then create a `Chat` object and call `.ask`!

``` dart
final chat = await nobodywho.Chat.fromPath(
  modelPath: 'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
);
final msg = await chat.ask('Is water wet?').completed();
print(msg); // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading. You can also have a look at our [flutter starter app repository](https://github.com/nobodywho-ooo/flutter-starter-example).

## Minimum recommended specs

- iOS: iPhone 11 or newer with at least 4 GB of RAM. We tested a Qwen3 0.6B (332 MB) on an iPhone X (iOS 16) and while it ran, performance was too slow to be practical.
- Android: Snapdragon 855 / Adreno 640 / 6 GB RAM or better. The same Qwen3 0.6B model performed notably better on a OnePlus 7 Pro (Android 12) than on the iPhone X tested above.

## Feedback & Contributions

We welcome your feedback and ideas!

- Bug Reports & Improvements: If you encounter a bug or have suggestions, please open an issue on our [Issues](https://github.com/nobodywho-ooo/nobodywho/issues) page.
- Feature Requests & Question: For new feature requests or general questions, join the discussion on our [Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) page.
