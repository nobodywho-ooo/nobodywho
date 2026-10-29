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
need two parts for making this work:

1. Multimodal LLM, so the LLM can consume image-tokens or/and audio-tokens
2. Projection model, which converts images to image-tokens or/and audio to audio-tokens

To find such a model, refer to the [HuggingFace Image-Text-to-Text](https://huggingface.co/models?pipeline_tag=image-text-to-text&library=gguf&sort=likes) section
and [Audio-Text-to-Text](https://huggingface.co/models?pipeline_tag=audio-text-to-text&sort=trending). Some models like Gemma 4 even manage both!
Usually, the projection model then includes `mmproj` in its name.

If you are unsure which ones to pick, or just want a reasonable default, you can try [Gemma 4](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf?download=true) with its [BF16 projection model](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/mmproj-BF16.gguf?download=true),
which can do both image and audio.

With the downloaded GGUFs, you can simply add the projection model when loading the model:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final model = await nobodywho.Model.load(
  modelPath: "./multimodal-model.gguf",
  projectionModelPath: "./mmproj.gguf",
);
final chat = nobodywho.Chat(
  model: model,
  systemPrompt: "You are a helpful assistant, that can hear and see stuff!",
);
```

:::info
The language model and projection model have to **fit** together, as they are trained together!
Unfortunately you can't just take projection model and a LLM that you like and expect them
to work together.
:::

## Composing a prompt object
With the model configured, all that is left is to compose the prompt and send it to the model.
That is done through `askWithPrompt`, which accepts a `Prompt` containing a list of `PromptPart` values.

```dart continuation
final response = await chat.askWithPrompt(nobodywho.Prompt([
  nobodywho.TextPart("Tell me what you see in the image and what you hear in the audio."),
  nobodywho.ImagePart("./dog.png"),
  nobodywho.AudioPart("./sound.mp3"),
])).completed(); // It's a dog and a penguin!
```

That should be it! Beware though, that consuming images and audio can quickly drain the context,
and larger context sizes may be needed for smooth usage.

## Media in a message list

`Prompt` is for `ask()`. When you pass a whole conversation to `complete()`, the same interleaving
is expressed as a list of content parts — the shape the OpenAI and Anthropic libraries use:

```dart continuation
await chat.complete([
  nobodywho.Message.user(content: nobodywho.partsContent([
    nobodywho.TextPart("Tell me what you see in the image."),
    nobodywho.ImagePart("./dog.png"),
    nobodywho.TextPart("Answer in one word."),
  ])),
]).completed();
```

`partsContent` takes the same `TextPart` / `ImagePart` / `AudioPart` values `Prompt` does, and the
order is the order the model sees them in. For text-only messages, `userMessage("hello")` stays the
short form.

## Media in a saved conversation

`getChatHistory()` records the file path of every image and audio clip in the content part it
belongs to, so a conversation containing media can be saved and replayed later. `complete()`
re-reads each file, so the model sees the images and audio again rather than a conversation with
holes in it. The files therefore have to still be where they were — if one cannot be read,
`complete()` throws instead of quietly answering without it.
