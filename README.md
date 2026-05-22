[![Discord](https://img.shields.io/discord/1308812521456799765?logo=discord&style=flat-square)](https://discord.gg/qhaMc2qCYB)
[![Matrix](https://img.shields.io/badge/Matrix-000?logo=matrix&logoColor=fff)](https://matrix.to/#/#nobodywho:matrix.org)
[![Mastodon](https://img.shields.io/badge/Mastodon-6364FF?logo=mastodon&logoColor=fff&style=flat-square)](https://mastodon.gamedev.place/@nobodywho)
[![Pub.dev Version](https://img.shields.io/pub/v/nobodywho?include_prereleases&style=flat-square&label=pub.dev)](https://pub.dev/packages/nobodywho)
[![PyPI Version](https://img.shields.io/pypi/v/nobodywho?style=flat-square&labelColor=%233775A9&color=%23FFD242)](https://pypi.org/project/nobodywho/)
[![npm Version](https://img.shields.io/npm/v/react-native-nobodywho?style=flat-square&logo=npm&label=npm)](https://www.npmjs.com/package/react-native-nobodywho)
[![Godot Engine](https://img.shields.io/badge/Godot-%23FFFFFF.svg?logo=godot-engine&style=flat-square)](https://godotengine.org/asset-library/asset/2886)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg?style=flat-square)](CODE_OF_CONDUCT.md) 
[![Docs](https://img.shields.io/badge/Docs-lightblue?style=flat-square)](https://docs.nobodywho.ooo)

<p align="center">
  <img src="assets/logo-nobodywho.png" alt="NobodyWho Logo" width="150"/>
</p>

<h1 align="center">NobodyWho</h1>

<p align="center">
  <strong> On-device AI for any device.</strong><br/>
  NobodyWho is an inference engine that lets you run LLMs locally and efficiently.
</p>

---

## ✨ Features

* **Run locally, offline** — no API keys needed or hidden fees
* **Run any chat LLM** — Gemma, Qwen, Mistral and more
* **Fast, type-safe tool calling** — automatically generates structured grammars from your function signatures, no schema writing needed
* **Multimodal input** — provide image and audio information to your LLM
* **Model downloading** — load models directly from [Hugging Face](https://huggingface.co/models?library=gguf&sort=trending) or any URL

## ⚡️ Under the Hood

* Conversation-aware preemptive context shifting — retain full conversation memory without any message length limits
* GPU-accelerated inference via Vulkan or Metal — runs fast on any OS
* Compatible with thousands of pre-trained LLMs — use any LLM in the GGUF format
* Powered by the wonderful [llama.cpp](https://github.com/ggml-org/llama.cpp)

---

## Platforms

We support Kotlin, Swift, Python, Flutter, React Native and Godot.

| Platform | Installation | Documentation |
|----------|-------------|---------------|
| **Kotlin** | [Maven Central](#kotlin) | [docs.nobodywho.ooo/kotlin](https://docs.nobodywho.ooo/kotlin/) |
| **Swift** | [SPM](#swift) | [docs.nobodywho.ooo/swift](https://docs.nobodywho.ooo/swift/) |
| **React Native** | [npm](#react-native) | [docs.nobodywho.ooo/react-native](https://docs.nobodywho.ooo/react-native/) |
| **Flutter** | [pub.dev](#flutter) | [docs.nobodywho.ooo/flutter](https://docs.nobodywho.ooo/flutter/) |
| **Python** | [pypi](#python) | [docs.nobodywho.ooo/python](https://docs.nobodywho.ooo/python/) |
| **Godot** | [AssetLib](#godot) | [docs.nobodywho.ooo/godot](https://docs.nobodywho.ooo/godot/install/) |

## Quick Start

### Kotlin

```kotlin
import ai.nobodywho.Chat

val chat = Chat.fromPath(
    modelPath = "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
)

val response = chat.ask("What is the capital of Denmark?").completed()
println(response) // The capital of Denmark is Copenhagen.
```

Add to your `build.gradle.kts`:

```kotlin
// Android
implementation("ai.nobodywho:nobodywho-android:0.1.0")

// Desktop JVM (Linux, macOS, Windows)
implementation("ai.nobodywho:nobodywho:0.1.0")
```

[Kotlin documentation](https://docs.nobodywho.ooo/kotlin/)

---

### Swift

```swift
import NobodyWho

let chat = try await Chat.fromPath(
    modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
)

let response = try await chat.ask("What is the capital of Denmark?").completed()
print(response) // The capital of Denmark is Copenhagen.
```

Add via Swift Package Manager:

```
https://github.com/nobodywho-ooo/nobodywho-swift.git
```

[Swift documentation](https://docs.nobodywho.ooo/swift/) - [GitHub](https://github.com/nobodywho-ooo/nobodywho-swift)

---

### React Native

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
});

const msg = await chat.ask("What is the capital of Denmark?").completed();
console.log(msg); // The capital of Denmark is Copenhagen.
```

Install via npm:

```
npm install react-native-nobodywho
```

[React Native documentation](https://docs.nobodywho.ooo/react-native/) - [npm](https://www.npmjs.com/package/react-native-nobodywho) - [starter example app](https://github.com/nobodywho-ooo/react-native-starter-example)

---

### Flutter

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

void main() async {
  await nobodywho.NobodyWho.init();

  final chat = await nobodywho.Chat.fromPath(
    modelPath: 'huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf',
  );

  final msg = await chat.ask('What is the capital of Denmark?').completed();
  print(msg); // The capital of Denmark is Copenhagen.
}
```

Install via pub.dev:

```
flutter pub add nobodywho
```

[Flutter documentation](https://docs.nobodywho.ooo/flutter/) - [pub.dev](https://pub.dev/packages/nobodywho) - [starter example app](https://github.com/nobodywho-ooo/flutter-starter-example)

---

### Python

```python
from nobodywho import Chat

chat = Chat("huggingface:NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf")

response = chat.ask("What is the capital of Denmark?").completed()
print(response) // The capital of Denmark is Copenhagen.
```

Install via pip:

```sh
pip install nobodywho
```

[Python documentation](https://docs.nobodywho.ooo/python/) - [pypi](https://pypi.org/project/nobodywho/)

---

## Godot

You can install it from inside the Godot editor: In Godot 4.5+, go to AssetLib and search for "NobodyWho".

...or you can grab a specific version from our [github releases page.](https://github.com/nobodywho-ooo/nobodywho/releases) You can install these zip files by going to the "AssetLib" tab in Godot and selecting "Import".

Make sure that the ignore asset root option is set in the import dialogue.

For further instructions on how to setup NobodyWho in Godot please refer to our [docs](https://docs.nobodywho.ooo/godot/install/).

---

## Documentation

[The documentation](https://docs.nobodywho.ooo) has everything you might want to know: https://docs.nobodywho.ooo/

## How to Help 

* ⭐ Star the repo and spread the word about NobodyWho!
* Join our [Discord](https://discord.gg/qhaMc2qCYB) or [Matrix](https://matrix.to/#/#nobodywho:matrix.org) communities
* Found a bug? Open an issue!
* Submit your own PR - contributions welcome
* Help improve docs, write tutorials and demos


### Can I export to HTML5 or iOS?

Desktop (Linux, macOS, Windows) is supported across all bindings. Android is supported on Kotlin, Godot, Flutter and React Native. iOS is supported on Swift, Flutter and React Native. visionOS and watchOS are supported via the Swift package.

Web exports will be a bit trickier to get right. See issue [#111](https://github.com/nobodywho-ooo/nobodywho/issues/111).


## Licensing

There has been some confusion about the licensing terms of NobodyWho. To clarify:

> Linking two programs or linking an existing software with your own work does not – at least under European law – produce a derivative or extend the coverage of the linked software licence to your own work. [[1]](https://interoperable-europe.ec.europa.eu/collection/eupl/licence-compatibility-permissivity-reciprocity-and-interoperability)

You are allowed to use this plugin in proprietary and commercial projects, free of charge.

If you distribute modified versions of the code *in this repo*, you must open source those changes.

Feel free to make proprietary projects using NobodyWho, but don't make a proprietary fork of NobodyWho.
