---
title: Getting started
description: How to setup NobodyWho in Swift
sidebar_title: Getting started
order: 0
---

## How do I get started?

Add NobodyWho to your project using Swift Package Manager. In Xcode, go to **File → Add Package Dependencies** and enter:

```
https://github.com/nobodywho-ooo/nobodywho-swift.git
```

Or add it to your `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/nobodywho-ooo/nobodywho-swift.git", from: "1.0.0")
]
```

Now you are ready to download a GGUF model you like - if you don't have a specific model in mind, try [this one](https://huggingface.co/NobodyWho/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q4_K_M.gguf). Read more about [model selection](../model-selection.md).

Once you have the `.gguf` file accessible to your app, the next step is to create a `Chat` and call `.ask`!

```swift
import NobodyWho

let chat = try await Chat.fromPath(modelPath: "/path/to/model.gguf")
let response = try await chat.ask("Is water wet?").completed()
print(response) // Yes, indeed, water is wet!
```

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.

## Platform requirements

- **iOS**: iPhone 11 or newer with at least 4 GB of RAM. Requires iOS 15+.
- **macOS**: Apple Silicon or Intel Mac with at least 8 GB of RAM. Requires macOS 13+.

GPU acceleration is enabled by default using Metal on Apple platforms.

## Feedback & Contributions

We welcome your feedback and ideas!

- Bug Reports & Improvements: If you encounter a bug or have suggestions, please open an issue on our [Issues](https://github.com/nobodywho-ooo/nobodywho/issues) page.
- Feature Requests & Questions: For new feature requests or general questions, join the discussion on our [Discussions](https://github.com/nobodywho-ooo/nobodywho/discussions) page.
