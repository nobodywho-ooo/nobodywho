---
title: Vision & Hearing
description:  Enabling models to ingest images and audio
sidebar_position: 3
---

Easily provide image and audio information to your LLM.

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
  nobodywho.Text("Tell me what you see in the image and what you hear in the audio."),
  nobodywho.Image("./dog.png"),
  nobodywho.Audio("./sound.mp3"),
])).completed(); // It's a dog and a penguin!
```

## In-memory media (no temp files)

Use `ImageBytes` for encoded image bytes already in memory (PNG/JPEG/etc.
from `rootBundle.load()`, `http.get().bodyBytes`, a Flutter canvas, etc.)
and `AudioPcm` for 16-bit PCM samples (live microphone capture from
`flutter_sound` / `mic_stream`, or after decoding an audio file yourself).
Same sibling-class pattern exists in every other binding.

```dart continuation
await chat.resetHistory();
final pngBytes = await File("./dog.png").readAsBytes();
final samples = Int16List(16000); // one second of silence at 16 kHz, for shape

final response3 = await chat.askWithPrompt(nobodywho.Prompt([
  nobodywho.Text("Describe the image."),
  nobodywho.ImageBytes(pngBytes),
  nobodywho.AudioPcm(samples, sampleRate: 16000),
])).completed();
```

:::info Audio sample rate
`AudioPcm` requires PCM samples to be at the **model's expected
sample rate**. For every current audio-capable multimodal LLM (Gemma 4,
Phi-4 multimodal, Qwen2-Audio, etc.) this is **16 kHz** — the default if you
omit `sampleRate`. Most microphone APIs capture at 44.1 or 48 kHz; resample
to 16 kHz before passing in. NobodyWho fails fast with a clear error if the
rate doesn't match, rather than silently returning garbled transcriptions.
:::

## Tips for multimodality
As with textual prompts, the format in which you supply the multimodal prompt can matter in certain
scenarios. If the model performs poorly, try to mess around with the order of supplying the text
and the multimodal files, or the descriptions you supply. For example, the following prompt may perform better than the previously presented one.

```dart continuation
await chat.resetHistory();
final response2 = await chat.askWithPrompt(nobodywho.Prompt([
  nobodywho.Text("Tell me what you see in the image."),
  nobodywho.Image("./dog.png"),
  nobodywho.Text("Also tell me what you hear in the audio"),
  nobodywho.Audio("./sound.mp3"),
])).completed();
```

Also, there is still a lot of variance between how the models internally process the images.
This, for example, causes differences in how quickly the model consumes context - for some models like Gemma 3, the number of tokens per image is constant; for others like Qwen 3, they scale with the size of the image. In that case, you can increase the context size if the resources allow:

```dart continuation
final chat2 = nobodywho.Chat(
  model: model,
  systemPrompt: "You are a helpful assistant.",
  contextSize: 8192,
);
```

Or, for example, preprocess your images with some kind of compression (sometimes even changing the image type helps).

Moreover, audio ingestion seems to be also reliant a lot on the data type of the projection model file - for gemma 4,
ingesting audio works the best on BF16, while other types reportedly struggle. We thus recommend sticking at least trying out different
projection model files, if the one you picked does not work.

As always with more niche models you can find bugs. If you stumble upon some of them, please be sure to [report them](https://github.com/nobodywho-ooo/nobodywho/issues), so we can fix the functionality.
