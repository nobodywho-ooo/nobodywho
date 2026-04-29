---
title: Getting started
description: How to setup NobodyWho in React Native
sidebar_title: Getting started
order: 0
---

## How do I get started?

First, install `react-native-nobodywho`.
```bash
npm install react-native-nobodywho
```

No additional initialization step is required — the native module is loaded automatically when you first import from the package.

Now you are ready to download a GGUF model you like - if you don't have a specific model in mind, try [this one](https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](../model-selection.md).

Once you have the `.gguf` file on the device, the next step is to create a `Chat` and call `.ask`!

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({ modelPath: "/path/to/model.gguf" });
const response = await chat.ask("Is water wet?").completed();
console.log(response); // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

## Tracking download progress

When loading a remote model (e.g. via a `huggingface:` or `https://` path), pass an `onDownloadProgress` option to observe the download. It receives `(downloaded, total)` byte counts, is throttled to roughly 10 Hz with a guaranteed final emit on completion, and is not called for cached or local files.

```typescript
const chat = await Chat.fromPath({
  modelPath: "huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
  onDownloadProgress: (downloaded, total) => {
    console.log(`${downloaded} / ${total} bytes`);
  },
});
```

To get a full overview of the functionality provided by NobodyWho, simply keep reading.

## Android requirements

If you use the x86_64 Android emulator for development, your app must set `minSdkVersion` to at least 31. This is due to a threading feature (ELF TLS) that the Rust runtime requires on x86_64. ARM64 devices (i.e. all real phones) work with any `minSdkVersion`.

No specific NDK version is required — NobodyWho ships prebuilt shared libraries, so your project's NDK version does not affect the Rust code.

## Minimum recommended specs

- iOS: iPhone 11 or newer with at least 4 GB of RAM. We tested a Qwen3 0.6B (332 MB) on an iPhone X (iOS 16) and while it ran, performance was too slow to be practical.
- Android: Snapdragon 855 / Adreno 640 / 6 GB RAM or better. The same Qwen3 0.6B model performed notably better on a OnePlus 7 Pro (Android 12) than on the iPhone X tested above.

## Feedback & Contributions

We welcome your feedback and ideas!

- Bug Reports & Improvements: If you encounter a bug or have suggestions, please open an issue on our [Issues](https://github.com/nobodywho-ooo/nobodywho/issues) page.
- Feature Requests & Questions: For new feature requests or general questions, join the discussion on our [Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) page.
