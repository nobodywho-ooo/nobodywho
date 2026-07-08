---
title: Multimodal Models
description: Enabling models to natively ingest images and audio via a projection model
sidebar_position: 3
---

A picture is worth a thousand words (or at least a thousand tokens).
With NobodyWho, you can easily provide image and audio information directly to a multimodal LLM.

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
Usually, the projection model includes `mmproj` in its name.

If you are unsure which ones to pick, or just want a reasonable default, you can try [Gemma 4](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf?download=true) with its [BF16 projection model](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/mmproj-BF16.gguf?download=true),
which can do both image and audio.

With the downloaded GGUFs, you can load them using `Chat.fromPath`:

```swift
import NobodyWho

let chat = try await Chat.fromPath(
    modelPath: "/path/to/vision-model.gguf",
    projectionModelPath: "/path/to/mmproj.gguf",
    systemPrompt: "You are a helpful assistant, that can hear and see stuff!"
)
```

Or load the model separately:

```swift
let model = try await Model.load(
    modelPath: "/path/to/vision-model.gguf",
    projectionModelPath: "/path/to/mmproj.gguf"
)
let chat = try Chat(model: model, systemPrompt: "You are a helpful assistant.")
```

:::info
The language model and projection model have to **fit** together, as they are trained together!
Unfortunately you can't just take a projection model and an LLM that you like and expect them
to work together.
:::

## Composing a prompt object

With the model configured, all that is left is to compose the prompt and send it to the model.
Use `Prompt` to build prompts that mix text, images, and audio, then pass them to `chat.ask()`:

```swift
let prompt = Prompt([
    Prompt.text("Tell me what you see in the image and what you hear in the audio."),
    Prompt.image("/path/to/dog.png"),
    Prompt.audio("/path/to/sound.mp3"),
])
let response = try await chat.ask(prompt).completed()
```

That should be it! Beware though, that consuming images and audio can quickly drain the context,
and larger context sizes may be needed for smooth usage.
