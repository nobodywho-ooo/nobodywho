---
title: Vision
description:  Enabling models to see images
sidebar_title: Vision
order: 3
---

A picture is a worth a thousand words (or tokens, as we like to see them).
With NobodyWho, you can easily provide image information to your LLM.

## Choosing a model
Not all models have inbuilt image capabilities. Generally, you will
need two parts for making this work:

1. Vision-Language (VL) LLM, so the LLM can consume image-tokens
2. Projection model, which converts image to image-tokens

To find such a model, refer to the [HuggingFace Image-Text-to-Text](https://huggingface.co/models?pipeline_tag=image-text-to-text&library=gguf&sort=likes) section.
Usually, the projection model is then includes `mmproj` in its name.

If you are unsure which ones to pick, or just want a reasonable default, you can try [Gemma 3 4b](https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/blob/main/gemma-3-4b-it-Q4_K_M.gguf) with its [F16 projection model](https://huggingface.co/unsloth/gemma-3-4b-it-GGUF/blob/main/mmproj-F16.gguf).

With the downloaded GGUF's, you can simply add the projection model as:

```python
from nobodywho import Model, Chat

model = Model("./model.gguf", image_model_path="./projection_model.gguf")
chat = Chat(
    model, system_prompt="You are a helpful assistant."
)
```

## Composing a prompt object
With the model configured, all that is left is to compose the prompt and send it to the model.
That is done through the `Prompt` object.
```python
from nobodywho import Text, Image, Prompt

prompt = Prompt([
    Text("Tell me what you see in the images."),
    Image("./dog.png"),
    Image("./penguin.png")
])

chat.ask(prompt).completed() # It's a dog and a penguin!
```

## Tips for multimodality
As with textual prompts, the format in which you supply the multimodal prompt can matter in certain
scenarios. If the model performs poorly, try to mess around with the order of supplying the text
and the images, or the descriptions you supply. For example, following prompt may perform better than the previously presented.

```python
prompt = Prompt([
    Text("Tell me what you see in the first image."),
    Image("./dog.png"),
    Text("Also tell me what you see in the second image.")
    Image("./penguin.png")
])
```

Also, there is still a lot of variance between how the models internally process the images.
This for example causes differences in how quickly does the model consume context - for some models like Gemma 3, the number of tokens per image is constant; for others like Qwen 3, they scale with the size of the image. In that case, you can increase the context size, if the resources allow you:
```python
chat = Chat(
    model, system_prompt="You are a helpful assistant.", n_ctx=4096
)
```
Or for example preprocess your images with some kind of compression (sometimes even changing the image type helps).

Nevertheless, with more niche models you can find bugs. If you stumble upon some of them, please be sure to [report them](https://github.com/nobodywho-ooo/nobodywho/issues), so we can fix the functionality.