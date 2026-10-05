# NobodyWho for .NET

Run LLMs locally and offline from C#. No API keys, no network connection, no data leaving the machine.

NobodyWho is powered by [llama.cpp](https://github.com/ggml-org/llama.cpp) and runs thousands of GGUF models from [Hugging Face](https://huggingface.co/models?library=gguf&sort=trending). It supports chat, streaming, tool calling, structured output, vision, embeddings, reranking, speech-to-text and text-to-speech.

## Installation

```bash
dotnet add package NobodyWho
```

Requires .NET 10. The package includes native libraries for:

| Platform | Runtime identifier | GPU acceleration |
|----------|--------------------|------------------|
| Windows x64 | `win-x64` | Vulkan |
| Linux x64 | `linux-x64` | Vulkan |
| Linux ARM64 | `linux-arm64` | Vulkan |
| macOS Apple silicon | `osx-arm64` | Metal |

## Quick start

```csharp
using NobodyWho;

using var chat = await Chat.FromPathAsync(
    "hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF/Qwen_Qwen3-0.6B-Q4_K_M.gguf",
    systemPrompt: "You are a helpful assistant.");

// Stream tokens as they are generated...
await foreach (var token in chat.Ask("Is water wet?"))
    Console.Write(token);

// ...or wait for the whole response.
string response = await chat.Ask("Is water wet?").CompletedAsync();
```

Models given as `hf://` paths or `https://` URLs are downloaded on first use and cached.

## Tool calling

Pass any delegate. Its parameter names and types become the tool's JSON schema, and describe parameters with `[Description]`:

```csharp
using System.ComponentModel;
using NobodyWho;

var weather = new Tool(
    "get_weather",
    "Get the current weather for a city",
    ([Description("The city name")] string city) => $"It is sunny in {city}.");

using var model = await Model.LoadAsync("model.gguf");
using var chat = new Chat(model, tools: [weather]);
Console.WriteLine(await chat.Ask("What's the weather in Copenhagen?").CompletedAsync());
```

## Documentation

Full documentation lives at [docs.nobodywho.ooo/csharp](https://docs.nobodywho.ooo/csharp/).

## License

[EUPL-1.2](https://github.com/nobodywho-ooo/nobodywho/blob/main/LICENSE)
