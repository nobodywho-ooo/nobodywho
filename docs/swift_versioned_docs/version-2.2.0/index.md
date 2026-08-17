---
title: Getting started
description: How to setup NobodyWho in Swift
sidebar_position: 0
---

## How do I get started?

Add NobodyWho to your project using Swift Package Manager. In Xcode, go to **File → Add Package Dependencies** and enter:

```
https://github.com/nobodywho-ooo/nobodywho-swift.git
```

Or add it to your `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/nobodywho-ooo/nobodywho-swift.git", from: "2.1.0")
]
```

Models can be loaded from a local file path, a Hugging Face repository using `hf://` URLs, or any `https://` URL. If you don't have a specific model in mind, try [this one](https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](/docs/model-selection).

```swift
import NobodyWho

// From a Hugging Face repository
let chat = try await Chat.fromPath(
    modelPath: "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
)

// From an HTTPS URL
let chat = try await Chat.fromPath(
    modelPath: "https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf"
)

// From a local file
let chat = try await Chat.fromPath(modelPath: "/path/to/model.gguf")
```

Once you have a `Chat`, call `.ask` to get a response!

```swift
let response = try await chat.ask("Is water wet?").completed()
print(response) // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.

## Platform requirements

- **iOS**: iPhone 11 or newer with at least 4 GB of RAM. Requires iOS 15+.
- **macOS**: Apple Silicon or Intel Mac with at least 8 GB of RAM. Requires macOS 13+.
- **visionOS**: Apple Vision Pro. Requires visionOS 1.0+.
- **watchOS**: Requires watchOS 10+. CPU-only (Metal is not available). Due to limited memory on Apple Watch, only very small models are practical.

GPU acceleration is enabled by default using Metal on all Apple platforms.

## Feedback & Contributions

We welcome your feedback and ideas!

- Bug Reports & Improvements: If you encounter a bug or have suggestions, please open an issue on our [Issues](https://github.com/nobodywho-ooo/nobodywho/issues) page.
- Feature Requests & Questions: For new feature requests or general questions, join the discussion on our [Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) page.
