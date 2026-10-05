---
title: Multimodal Models
description: Enabling models to natively ingest images and audio via a projection model
sidebar_position: 3
---

Easily provide image and audio information directly to a multimodal LLM.

:::info
This is about models that **natively** ingest images and audio - no transcription step involved.
That matters for audio in particular: the model hears the raw sound, not just words that were said,
so it can react to tone of voice, music, or other non-speech noises. If you only need to convert
speech to text, see [Speech-to-Text](./speech-to-text) instead. If you need to generate spoken audio
from text, see [Text-to-Speech](./text-to-speech).
:::

## Choosing a model
Not all models have built-in image and audio capabilities. Generally, you will
need two parts:

1. Multimodal LLM that can consume image-tokens and/or audio-tokens
2. Projection model that converts images to image-tokens and/or audio to audio-tokens

To find such a model, refer to the [HuggingFace Image-Text-to-Text](https://huggingface.co/models?pipeline_tag=image-text-to-text&library=gguf&sort=likes) section
and [Audio-Text-to-Text](https://huggingface.co/models?pipeline_tag=audio-text-to-text&sort=trending). Some models like Gemma 4 manage both!
Usually, the projection model includes `mmproj` in its name.

If you are unsure which ones to pick, try [Gemma 4](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf?download=true) with its [BF16 projection model](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/mmproj-BF16.gguf?download=true).

Load the projection model alongside the main model:

```csharp
using NobodyWho;

using var model = await Model.LoadAsync(
    "./multimodal-model.gguf",
    projectionModelPath: "./mmproj.gguf");
using var chat = new Chat(model);
```

`Chat.FromPathAsync` takes the same `projectionModelPath` parameter.

:::info
The language model and projection model must **fit** together, as they are trained together.
You can't take an arbitrary projection model and pair it with any LLM.

:::

## Composing a prompt

With the model configured, compose a multimodal prompt using `Prompt`:

```csharp
string response = await chat.Ask(new Prompt(
    new ContentPart.Text("Tell me what you see in the image and what you hear in the audio."),
    new ContentPart.Image("./dog.png"),
    new ContentPart.Audio("./sound.mp3")
)).CompletedAsync();
Console.WriteLine(response); // It's a dog!
```

That should be it! Beware though, that consuming images and audio can quickly drain the context,
and larger context sizes may be needed for smooth usage. `chat.TokenizeAsync(prompt)` shows how
much room a prompt takes up: text produces token IDs, and each image or audio embedding slot
produces `null`.

## Media in a message list

`Prompt` is for `Ask()`. When you pass a whole conversation to `Complete()`, the same interleaving
is expressed as a list of content parts — the shape the OpenAI and Anthropic libraries use:

```csharp
await chat.Complete([
    new Message.User(new MessageContent.Parts(
        new ContentPart.Text("Tell me what you see in the image."),
        new ContentPart.Image("./dog.png"),
        new ContentPart.Text("Answer in one word."))),
]).CompletedAsync();
```

Parts are `ContentPart.Text`, `ContentPart.Image` and `ContentPart.Audio`. The order is the order
the model sees them in. A plain string converts to `MessageContent` implicitly, so it stays valid
wherever content is accepted (`new Message.User("hello")`), and text-only messages need no change.

## Media in a saved conversation

`GetChatHistoryAsync()` records the file path of every image and audio clip in the content part it
belongs to, so a conversation containing media can be saved and replayed later. Messages serialize
with `System.Text.Json`:

```csharp
using System.Text.Json;

// save it, maybe to a save file
string saved = JsonSerializer.Serialize(await chat.GetChatHistoryAsync());

// ... later, in a new process ...
var history = JsonSerializer.Deserialize<Message[]>(saved)!;
using var newChat = new Chat(model);
string answer = await newChat.Complete([
    .. history,
    new Message.User("What colour was the dog?"),
]).CompletedAsync();
```

`Complete()` re-reads each file, so the model sees the images and audio again rather than a
conversation with holes in it. The files therefore have to still be where they were — if one
cannot be read, `Complete()` throws instead of quietly answering without it.
