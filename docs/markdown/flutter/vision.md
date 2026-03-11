---
title: Vision
description:  Enabling models to see images
sidebar_title: Vision
order: 3
---

A picture is worth a thousand words (or at least a thousand tokens).
With NobodyWho, you can easily provide image information to your LLM.

## Choosing a model
Not all models have built-in image capabilities. Generally, you will
need two parts for making this work:

1. Vision-Language (VL) LLM, so the LLM can consume image-tokens
2. Projection model, which converts images to image-tokens

To find such a model, refer to the [HuggingFace Image-Text-to-Text](https://huggingface.co/models?pipeline_tag=image-text-to-text&library=gguf&sort=likes) section.
Usually, the projection model then includes `mmproj` in its name.

If you are unsure which ones to pick, or just want a reasonable default, you can try [Gemma 3 4b](https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/blob/main/gemma-3-4b-it-Q4_K_M.gguf) with its [F16 projection model](https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/blob/main/mmproj-F16.gguf).

With the downloaded GGUFs, you can simply add the projection model when loading the model:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final model = await nobodywho.Model.load(
  modelPath: "./model.gguf",
  imageIngestion: "./mmproj.gguf",
);
final chat = nobodywho.Chat(
  model: model,
  systemPrompt: "You are a helpful assistant.",
);
```

## Composing a prompt object
With the model configured, all that is left is to compose the prompt and send it to the model.
That is done through `askWithPrompt`, which accepts a list of `PromptPart` values.

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

final response = await chat.askWithPrompt([
  nobodywho.PromptPart.text(content: "Tell me what you see in the images."),
  nobodywho.PromptPart.image(path: "./dog.png"),
  nobodywho.PromptPart.image(path: "./penguin.png"),
]).completed(); // It's a dog and a penguin!
```

## Tips for multimodality
As with textual prompts, the format in which you supply the multimodal prompt can matter in certain
scenarios. If the model performs poorly, try to mess around with the order of supplying the text
and the images, or the descriptions you supply. For example, the following prompt may perform better than the previously presented one.

```dart
final response = await chat.askWithPrompt([
  nobodywho.PromptPart.text(content: "Tell me what you see in the first image."),
  nobodywho.PromptPart.image(path: "./dog.png"),
  nobodywho.PromptPart.text(content: "Also tell me what you see in the second image."),
  nobodywho.PromptPart.image(path: "./penguin.png"),
]).completed();
```

Also, there is still a lot of variance between how the models internally process the images.
This, for example, causes differences in how quickly the model consumes context - for some models like Gemma 3, the number of tokens per image is constant; for others like Qwen 3, they scale with the size of the image. In that case, you can increase the context size if the resources allow:

```dart
final chat = nobodywho.Chat(
  model: model,
  systemPrompt: "You are a helpful assistant.",
  contextSize: 8192,
);
```

Or, for example, preprocess your images with some kind of compression (sometimes even changing the image type helps).

Nevertheless, with more niche models you can find bugs. If you stumble upon some of them, please be sure to [report them](https://github.com/nobodywho-ooo/nobodywho/issues), so we can fix the functionality.
