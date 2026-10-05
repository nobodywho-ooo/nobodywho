---
title: Chat
description: A concise introduction to the Chat functionality of NobodyWho.
sidebar_position: 1
---

Every interaction with your LLM starts with a `Chat` object.

## Creating a Chat

The simplest way is using `Chat.FromPathAsync`:

```csharp
using var chat = await Chat.FromPathAsync("./model.gguf");
```

This is an async method since loading a model can take a bit of time, but it won't block your UI thread. Pass a `CancellationToken` as `cancellationToken` to stop waiting for a load you no longer need. A chat created this way owns its model and frees it when the chat is disposed.

Another way is to load the model separately and share it between multiple chats:

```csharp
using var model = await Model.LoadAsync("./model.gguf");
using var chat1 = new Chat(model);
using var chat2 = new Chat(model);
```

NobodyWho takes care of the separation, so your chat histories won't collide or interfere with each other. Each chat keeps its own reference to the model, so disposing the `Model` does not break chats that are still using it.

You can pass `"auto"` as the model path to select a chat model based on available memory.

## Prompts and responses

The `Ask()` method sends your message to the LLM, which then starts generating a response:

```csharp
using var chat = await Chat.FromPathAsync("./model.gguf");
TokenStream stream = chat.Ask("Is water wet?");
```

The return type is a `TokenStream`, which is an `IAsyncEnumerable<string>`. If you want to read the response as it generates, use `await foreach`:

```csharp
await foreach (var token in chat.Ask("Is water wet?"))
{
    Console.Write(token);
}
```

If you just want the complete response, call `CompletedAsync()`:

```csharp
string fullResponse = await chat.Ask("Is water wet?").CompletedAsync();
```

You can also pull one token at a time with `NextTokenAsync()`, which returns `null` once the response is complete.

All messages and responses are stored in the `Chat`, so the next `Ask()` remembers the conversation.

### Structured prompts

`Ask()` also takes a `Prompt`. Besides the images and audio covered in [Multimodal Models](./vision), a prompt can hold JSON. `Prompt.FromJson` serializes any value with `System.Text.Json`, and chat templates written for structured content receive it as a real list or map rather than as a string:

```csharp
var prompt = Prompt.FromJson(new { question = "What is 2 + 2?", answerFormat = "number" });
string answer = await chat.Ask(prompt).CompletedAsync();
```

`Prompt.FromJsonText` does the same for a JSON string you already have.

## Stopping generation

If you need to cancel the model's response while it is still generating (for example, when the user clicks a "Stop" button), pass a `CancellationToken`:

```csharp
using var cts = new CancellationTokenSource(TimeSpan.FromSeconds(10));

try
{
    await foreach (var token in chat.Ask("Write a long story.").WithCancellation(cts.Token))
    {
        Console.Write(token);
    }
}
catch (OperationCanceledException)
{
    Console.WriteLine("\n[stopped]");
}
```

`CompletedAsync(cancellationToken)` and `NextTokenAsync(cancellationToken)` take a token too. Cancelling stops generation and throws `OperationCanceledException`; the chat stays usable. You can also call `chat.StopGeneration()` directly, from any thread. The partial response is added to the chat history, so the conversation remains coherent.

## Chat history

Inspect the messages inside the `Chat`:

```csharp
IReadOnlyList<Message> history = await chat.GetChatHistoryAsync();
foreach (var message in history)
{
    Console.WriteLine($"{message.GetType().Name}: {message.Content.PlainText}");
}
```

Messages are records: `Message.User`, `Message.Assistant` (with any `ToolCalls` it made), `Message.System` and `Message.Tool`, so you can pattern-match on them. `Content.PlainText` gives a message's text.

Or set the history directly:

```csharp
await chat.SetChatHistoryAsync([
    new Message.User("What is water?"),
]);
```

## Chat completion

If you would rather pass the whole conversation on every call than let the `Chat` remember it, use `Complete()`:

```csharp
string response = await chat.Complete([
    new Message.System("You are a helpful assistant."),
    new Message.User("Who was the first person to walk on the moon?"),
    new Message.Assistant("Neil Armstrong."),
    new Message.User("Which year did he do it?"),
]).CompletedAsync();
```

You get back the same `TokenStream` as from `Ask()`.

The list you pass **becomes** the chat history, replacing whatever was there, and the response is added to it — so `Ask()` continues that same conversation. A system message at the front sets the chat's system prompt; leave it out and the prompt already on the chat is kept.

A system message further in stays in the history, for the chat template to render in place. Not every model has a system role — those fold the system prompt into the first user message instead, and only a leading one can be folded, so generating throws an error telling you where to move the instruction.

The list must not be empty and must end in a user or tool message. Anything else throws a `NobodyWhoException`.

### Per-turn settings

`Complete()` takes an optional `CompletionOptions` carrying the chat's other settings. It follows the same rule as the system message: what it sets stays set, what it leaves out (`null`) is kept.

```csharp
await chat.Complete(
    [new Message.User("Name one fruit.")],
    new CompletionOptions(
        Sampler: SamplerPresets.Greedy(),
        TemplateVariables: new Dictionary<string, bool> { ["enable_thinking"] = false })
).CompletedAsync();

// Both are now the chat's settings, so the next call need not repeat them
await chat.Complete([new Message.User("Name another.")]).CompletedAsync();
```

Fill in all three fields (`Sampler`, `TemplateVariables` and `Tools`) and the call no longer depends on what the chat is currently holding — useful if you drive it entirely through `Complete()`.

Changing `Tools` re-selects the chat template and rewrites the system-prompt region, so that turn re-prefills from near token zero. Set it when it changes, not on every call.

## System prompt

A system prompt guides the model's overall behavior. Some models ship with a built-in default.

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    systemPrompt: "You are a mischievous assistant!");
```

The system prompt persists until the chat context is reset. You can read and change it with `GetSystemPromptAsync()` and `SetSystemPromptAsync()`.

## Context

The context is the token window the LLM currently considers. Larger context means more computational overhead:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    contextSize: 4096);
```

The default is `4096`. You can check the maximum context size the model was trained with using `model.MaxContextSize` — setting `contextSize` above this value has no benefit.

When a conversation fills the context window, NobodyWho removes older turns until the context is below half of `contextSize`. A turn includes a user message and everything before the next user message.

System messages are always kept. By default, NobodyWho also keeps the first turn and the last two turns. The KV cache is updated automatically.

Use `ContextShiftOptions` to adjust this behavior:

- `Target`: Remove turns until the context is below this limit. Use `new ShiftTarget.Fraction(...)` with a value between 0 and 1 for a fraction of `contextSize`, or `new ShiftTarget.Tokens(...)` with a count below `contextSize` for a token count. Defaults to half of `contextSize`. Whole turns are removed, so the resulting size may be smaller. A higher value, such as 0.9, removes fewer turns but may trigger more frequent shifts.
- `KeepFirstTurns`: Number of initial turns to keep. Defaults to 1. Increase this to preserve more of the opening conversation.
- `KeepLastTurns`: Number of recent turns to keep. Defaults to 2. Must be at least 1, so the message being answered is always kept.

Example:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    contextSize: 4096,
    contextShift: new ContextShiftOptions(
        KeepFirstTurns: 1,
        KeepLastTurns: 4,
        Target: new ShiftTarget.Fraction(0.75f)));
```

To turn context shifting off, pass `new ContextShiftOptions(Enabled: false)`; a full context then throws instead. The options can also be changed on an existing chat:

```csharp
await chat.SetContextShiftAsync(new ContextShiftOptions(Enabled: false));
```

To reset the context with a new system prompt and tools:

```csharp
await chat.ResetContextAsync(systemPrompt: "New system prompt", tools: []);
```

To just clear the history without changing settings:

```csharp
await chat.ResetHistoryAsync();
```

To inspect how much of the context is currently in use, call `GetStatsAsync()`:

```csharp
ChatStats stats = await chat.GetStatsAsync();
Console.WriteLine($"Using {stats.ContextUsed} of {stats.ContextSize} tokens");
```

To see how many tokens a piece of text will take up, `TokenizeAsync()` tokenizes it with the chat's model:

```csharp
IReadOnlyList<int> tokens = await chat.TokenizeAsync("How long is this?");
Console.WriteLine($"{tokens.Count} tokens");
```

## CPU threads

When layers run on the CPU, NobodyWho picks a thread count for you: one per *performance* core,
not one per logical CPU. Efficiency cores end up pacing the whole thread pool, so using every
CPU is usually slower — see [LLM Basics](/docs/llm-basics#cpu-threads) for the numbers.

Override it with `threadCount` when you want to leave CPU headroom for the rest of your app —
for example a desktop app whose UI should stay smooth while the model is generating:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    threadCount: 4);
```

Leave it unset — or pass `null` — to keep the detected default. Values above the device's CPU
count are clamped, and it has little effect when the model is offloaded to the GPU.

## GPU

When loading a model, GPU acceleration is enabled by default:

```csharp
using var model = await Model.LoadAsync("./model.gguf", useGpu: true);
```

NobodyWho uses Vulkan on Linux/Windows and Metal on macOS for GPU acceleration.

## Speculative decoding (MTP)

Some models come with **MTP** (Multi-Token Prediction) draft heads that let the target model verify several candidate tokens per forward pass. See [LLM Basics](/docs/llm-basics#speculative-decoding-mtp) for the underlying idea.

Load the model with a compatible draft-heads gguf (e.g. `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B) and pass an `MtpConfig` when constructing the chat:

```csharp
using var model = await Model.LoadAsync(
    "./gemma-4-e2b.gguf",
    draftModelPath: "./mtp-gemma-4-e2b.gguf");

// new MtpConfig() uses the default drafter tuning; tune with new MtpConfig(KMax: ..., PMin: ...)
using var chat = new Chat(model, mtp: new MtpConfig());
```

`Chat.FromPathAsync` accepts the same two parameters if you don't need to share the model:

```csharp
using var chat = await Chat.FromPathAsync(
    "./gemma-4-e2b.gguf",
    draftModelPath: "./mtp-gemma-4-e2b.gguf",
    mtp: new MtpConfig());
```

Loading the draft heads adds around 5% to VRAM usage.

After a response, `GetMtpAcceptanceRateAsync()` returns the share of drafted tokens the model accepted (between 0 and 1), or `null` when no drafts were proposed. A low rate means MTP is costing more than it saves.

:::warning
Benchmark before enabling. MTP can hurt performance on Apple Silicon (Metal) and on high-entropy workloads like creative prose.
:::

## Template Variables

Chat templates are used internally by models to format conversation history. Template variables are boolean flags that control specific behaviors.

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    templateVariables: new Dictionary<string, bool> { ["enable_thinking"] = true });
```

You can also modify them on an existing chat:

```csharp
await chat.SetTemplateVariableAsync("enable_thinking", false);
IReadOnlyDictionary<string, bool> variables = await chat.GetTemplateVariablesAsync();
```

### Example: Qwen3 Reasoning

The Qwen3 model family supports `enable_thinking`, which controls whether the model shows its reasoning process before answering:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    templateVariables: new Dictionary<string, bool> { ["enable_thinking"] = true });

string response = await chat.Ask("Solve this logic puzzle: ...").CompletedAsync();
```

:::info
Template variables are model-specific. If a model's chat template doesn't use a specific variable, it will be ignored.

:::
