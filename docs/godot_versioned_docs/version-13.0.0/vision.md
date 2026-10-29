---
title: Multimodal Models
description: Enabling models to natively ingest images and audio via a projection model
sidebar_position: 3
---

Some models can see pictures and listen to audio: feed them a screenshot of the game world, a
portrait of an NPC, or a player's voice recording, and they answer questions about it.

## Choosing a model

Multimodal support needs two files: the multimodal LLM itself (a `.gguf`), and a **projection
model** (an "mmproj" `.gguf`) that translates images/audio into the token space the LLM
understands. For example, for
[`unsloth/gemma-3-4b-it-GGUF`](https://huggingface.co/unsloth/gemma-3-4b-it-GGUF) you want
`gemma-3-4b-it-Q4_K_M.gguf` together with `mmproj-F16.gguf`.

Load the model with the projector attached, then create the chat from it:

```gdscript
var model = await NobodyWhoModel.create("./gemma-3-4b-it.gguf", {
    "mmproj_path": "./mmproj-F16.gguf",
})
var chat = await NobodyWhoChat.create(model, {})
```

## Composing a prompt object

A multimodal prompt mixes text with media parts. Build the parts with the `NobodyWhoPrompt`
factories, and combine them with `NobodyWhoPrompt.create`:

```gdscript
var prompt = await NobodyWhoPrompt.create([
    NobodyWhoPrompt.text("What's in this picture?"),
    NobodyWhoPrompt.image("res://dog.png"),
])
var stream = chat.ask(prompt)
print(await stream.completed())
```

Audio works the same way with `NobodyWhoPrompt.audio(path)`. Paths accept `res://` and `user://`
like everywhere else in NobodyWho. `create` resolves to a `NobodyWhoPrompt` you pass to `ask()`,
or `null` (with a `godot_error!`) if a part is malformed.

## Media in a message list

`set_chat_history()` accepts media too. Instead of a plain string, give a message's `"content"`
an array of parts, each a dictionary with `"type"` and the part's payload:

```gdscript
await chat.set_chat_history([
    {
        "role": "user",
        "content": [
            {"type": "text", "text": "Remember this dog."},
            {"type": "image", "path": "res://dog.png"},
        ],
    },
])
```

`res://` and `user://` media paths are globalized before the files are loaded.
`get_chat_history()` returns media messages in the same shape, with those paths stored as absolute
filesystem paths, so the history can be saved and loaded again.

## Media in a saved conversation

If your save file stores conversation state as JSON, `NobodyWhoPrompt.from_json` rebuilds a
prompt from any JSON-compatible value:

```gdscript
# at save time — a prompt object serializes through your own save logic;
# a history from get_chat_history() already is plain data, so just store it.
var saved: Array = await chat.get_chat_history()

# ...later, in a new session:
await chat.set_chat_history(saved)
```

For a single prompt, round-trip it explicitly:

```gdscript
var prompt = await NobodyWhoPrompt.from_json(saved_prompt_dict)
var stream = chat.ask(prompt)
```

:::info
Saved histories contain globalized media paths. They remain valid on the same installation, but
may need to be rewritten if a save is moved to another machine or installation directory.
:::
