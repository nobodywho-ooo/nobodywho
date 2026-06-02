---
title: Vision & Hearing
description: Enabling models to ingest images and audio
sidebar_position: 3
---

Easily provide image and audio information to your LLM.

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

```kotlin
import ai.nobodywho.Model
import ai.nobodywho.Chat

val model = Model.load(
    modelPath = "./multimodal-model.gguf",
    projectionModelPath = "./mmproj.gguf"
)
val chat = Chat(model = model)
```

:::info
The language model and projection model must **fit** together, as they are trained together.
You can't take an arbitrary projection model and pair it with any LLM.

:::

## Composing a prompt

With the model configured, compose a multimodal prompt using `Prompt`:

```kotlin
import ai.nobodywho.Prompt

val response = chat.ask(Prompt(
    Prompt.Text("Tell me what you see in the image and what you hear in the audio."),
    Prompt.Image("./dog.png"),
    Prompt.Audio("./sound.mp3"),
)).completed()
println(response) // It's a dog!
```

## Tips for multimodality

The format in which you supply the multimodal prompt can matter. If the model performs poorly, try changing the order of text and media, or adjusting descriptions:

```kotlin
chat.resetHistory()
val response = chat.ask(Prompt(
    Prompt.Text("Tell me what you see in the image."),
    Prompt.Image("./dog.png"),
    Prompt.Text("Also tell me what you hear in the audio."),
    Prompt.Audio("./sound.mp3"),
)).completed()
```

Different models process images differently — some use a fixed number of tokens per image, others scale with image size. You may need to increase the context size:

```kotlin
val chat = Chat(
    model = model,
    contextSize = 8192u
)
```

For large images, consider downsizing before sending to the model to reduce processing time, especially on mobile devices.
