---
title: Getting started
description: How to setup NobodyWho in React Native / Expo
sidebar_position: 0
---

## How do I get started?

First, install `react-native-nobodywho`.
```bash
# React Native
npm install react-native-nobodywho
# Expo
npx expo install react-native-nobodywho
```

### React Native

No additional initialization step is required — the native module is loaded automatically when you first import from the package.

### Expo

NobodyWho ships native code, so it does **not** run in [Expo Go](https://docs.expo.dev/get-started/set-up-your-environment/). You need a [development build](https://docs.expo.dev/develop/development-builds/introduction/).

#### • Continuous Native Generation (CNG) projects

In a managed project, `ios/` and `android/` are not committed. For the **first build**, run the command for the platform you are targeting (`run:ios` and `run:android` are alternatives — pick your target). It runs prebuild automatically since the native folders don't exist yet, installs pods, and builds NobodyWho in:

```bash
npx expo run:ios
# or
npx expo run:android
```

After you **upgrade** NobodyWho (or any other native dependency), the native folders already exist, so `run:*` would build against stale native code. Regenerate them from scratch, then rebuild:

```bash
npx expo prebuild --clean   # regenerate ios/ and android/ from scratch
npx expo run:ios            # then rebuild (or run:android)
```

#### • Bare projects

Here `ios/` and `android/` are committed. Autolinking registers the module on your next native build — just rebuild with `expo run:*` (or your existing native build). Do not run `prebuild --clean` here unless you intend to regenerate the native folders, since it overwrites manual native edits.

You can also build with [EAS Build](https://docs.expo.dev/build/introduction/) instead of building locally. No config plugin is required.

## Pick a model

Now you are ready to pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass a `huggingface:` path. See [model selection](/docs/model-selection) for recommendations.

Then create a `Chat` object and call `.ask`!

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
});
const response = await chat.ask("Is water wet?").completed();
console.log(response); // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

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
