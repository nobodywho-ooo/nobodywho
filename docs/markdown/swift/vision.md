---
title: Vision & Hearing
description: Enabling models to ingest images and audio
sidebar_title: Vision & Hearing
order: 3
---

A picture is worth a thousand words (or at least a thousand tokens).
With NobodyWho, you can easily provide image and audio information to your LLM.

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
let chat = Chat(model: model, systemPrompt: "You are a helpful assistant.")
```

!!! info ""
    The language model and projection model have to **fit** together, as they are trained together!
    Unfortunately you can't just take a projection model and an LLM that you like and expect them
    to work together.

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

## Tips for multimodality

As with textual prompts, the format in which you supply the multimodal prompt can matter in certain
scenarios. If the model performs poorly, try to mess around with the order of supplying the text
and the multimodal files, or the descriptions you supply. For example, the following prompt may perform better than the previously presented one.

```swift
let prompt = Prompt([
    Prompt.text("Tell me what you see in the image."),
    Prompt.image("/path/to/dog.png"),
    Prompt.text("Also tell me what you hear in the audio."),
    Prompt.audio("/path/to/sound.mp3"),
])
```

Also, there is still a lot of variance between how the models internally process the images.
This, for example, causes differences in how quickly the model consumes context - for some models like Gemma 3, the number of tokens per image is constant; for others like Qwen 3, they scale with the size of the image. In that case, you can increase the context size if the resources allow:

```swift
let chat = try await Chat.fromPath(
    modelPath: "/path/to/vision-model.gguf",
    projectionModelPath: "/path/to/mmproj.gguf",
    contextSize: 8192
)
```

Or, for example, preprocess your images with some kind of downsampling (sometimes even changing the image type helps).

Moreover, audio ingestion seems to be also reliant a lot on the data type of the projection model file - for gemma 4,
ingesting audio works the best on BF16, while other types reportedly struggle. We thus recommend at least trying out different
projection model files, if the one you picked does not work.

As always with more niche models you can find bugs. If you stumble upon some of them, please be sure to [report them](https://github.com/nobodywho-ooo/nobodywho/issues), so we can fix the functionality.
