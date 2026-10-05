---
title: Getting started
description: How to setup NobodyWho in C#
sidebar_position: 0
---

## How do I get started?

First, add NobodyWho to your project:

```bash
dotnet add package NobodyWho
```

NobodyWho requires .NET 10. The package ships native libraries for these platforms:

| Platform | Runtime identifier | GPU acceleration |
| -------- | ------------------ | ---------------- |
| Windows x64 | `win-x64` | Vulkan |
| Linux x64 | `linux-x64` | Vulkan |
| Linux ARM64 | `linux-arm64` | Vulkan |
| macOS Apple silicon | `osx-arm64` | Metal |

Next, pick a model. NobodyWho can download GGUF models directly from Hugging Face — just pass an `hf://` path. See [model selection](/docs/model-selection) for recommendations.

Then create a `Chat` and call `Ask`!

```csharp
using NobodyWho;

using var chat = await Chat.FromPathAsync(
    "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf");

// stream tokens as they are generated
await foreach (var token in chat.Ask("Is water wet?"))
{
    Console.Write(token);
}

// ...or get the entire response as a single string
string response = await chat.Ask("Is water wet?").CompletedAsync();
Console.WriteLine(response);
```

Everything slow — loading models, generating, encoding — is `async`, so in a WPF, WinForms, Avalonia or ASP.NET app you just `await` it and your UI or request threads stay free.

`Chat`, `Model` and the other NobodyWho types hold native memory, so they implement `IDisposable`. Create them with `using` (as above) or call `Dispose()` when you are done with them. Errors from the native library are thrown as `NobodyWhoException`.

The examples in the rest of these docs assume `using NobodyWho;` and run inside an `async` method (top-level statements work too).

This is a super simple example, but we believe that examples which do simple things, should be simple!

To get a full overview of the functionality provided by NobodyWho, simply keep reading.
