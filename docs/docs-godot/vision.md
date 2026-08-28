---
title: Multimodal Models
description: Enabling models to natively ingest images and audio via a projection model
sidebar_position: 7
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
Usually, the projection model then includes `mmproj` in its name.

If you are unsure which ones to pick, or just want a reasonable default, you can try [Gemma 4](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/gemma-4-E2B-it-Q4_K_M.gguf?download=true) with its [BF16 projection model](https://huggingface.co/unsloth/gemma-4-E2B-it-GGUF/resolve/main/mmproj-BF16.gguf?download=true),
which can do both image and audio.

With the downloaded GGUFs, you can set the projection model on your `NobodyWhoModel` node.
In the editor, set the `projection_model_path` property to point to your projection model file.
Alternatively, you can set it in GDScript:

```gdscript
$ChatModel.projection_model_path = "res://mmproj.gguf"
```

:::info
The language model and projection model have to **fit** together, as they are trained together!
Unfortunately you can't just take projection model and a LLM that you like and expect them
to work together.
:::

## Composing a prompt object
With the model configured, all that is left is to compose the prompt and send it to the model.
That is done through the `NobodyWhoPrompt` object.

```gdscript
extends NobodyWhoChat

func _ready():
    self.model_node = get_node("../ChatModel")
    self.system_prompt = "You are a helpful assistant, that can hear and see stuff!"

    var prompt = NobodyWhoPrompt.new()
    prompt.add_text("Tell me what you see in the image and what you hear in the audio.")
    prompt.add_image("res://dog.png")
    prompt.add_audio("res://sound.mp3")

    ask(prompt)
    var response = await response_finished  # It's a dog and a penguin!
```

That should be it! Beware though, that consuming images and audio can quickly drain the context,
and larger context sizes may be needed for smooth usage.

## Media in a message list

`NobodyWhoPrompt` is for `ask()`. When you pass a whole conversation to `complete()`, the same
interleaving is expressed as a list of content parts — the shape the OpenAI and Anthropic libraries
use:

```gdscript
complete([
    {"role": "user", "content": [
        {"type": "text",  "text": "Tell me what you see in the image."},
        {"type": "image", "path": "./dog.png"},
        {"type": "text",  "text": "Answer in one word."},
    ]},
])
var response = await response_finished
```

Parts are `{"type": "text", "text": ...}`, `{"type": "image", "path": ...}` and
`{"type": "audio", "path": ...}`. The order is the order the model sees them in. A plain String
stays valid wherever a content list is accepted, so text-only messages need no change.

## Media in a saved conversation

`get_chat_history()` records the file path of every image and audio clip in the content part it
belongs to, so a conversation containing media can be saved and replayed later. `complete()`
re-reads each file, so the model sees the images and audio again rather than a conversation with
holes in it. The files therefore have to still be where they were — if one cannot be read,
`complete()` emits `worker_failed` instead of quietly answering without it.
