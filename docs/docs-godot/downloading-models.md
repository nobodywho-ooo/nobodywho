---
title: Downloading models
description: Where to put your model files and how NobodyWho downloads them
sidebar_position: 1
---

NobodyWho needs a model file to do anything useful. Model paths are accepted everywhere a model is
loaded — `NobodyWhoChat.create()`, `NobodyWhoModel.create()`, `NobodyWhoEncoder.create()`, and so
on — and all of them accept the same set of formats.

## Supported model path formats

- **Local file path**: `"./model.gguf"`, `"C:/models/model.gguf"`, etc.
- **Godot paths**: `"res://models/model.gguf"` (shipped with your game) or
  `"user://models/model.gguf"` (writable per-user directory). NobodyWho resolves these to real
  filesystem paths for you.
- **Hugging Face repos**: `"hf://owner/repo"` — a model from Hugging Face, downloaded and cached
  on first use. Examples:
  - `"hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF"` (chat, GGUF)
  - `"hf://onnx-community/whisper-base"` (speech to text, ONNX)
- **`"auto"`**: pick a chat model that fits the machine's available memory.

```gdscript
var chat = await NobodyWhoChat.create("hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF", {})
```

Downloads go to a per-user cache directory: `$XDG_CACHE_HOME/nobodywho/models` on Linux, the
equivalent `~/.cache` location on macOS, and `%LOCALAPPDATA%\nobodywho\models` on Windows. The
cache survives restarts, so each model is downloaded once. If you ship a model inside your game
with `res://`, nothing is ever downloaded.

## Downloading a gated model

Some Hugging Face repositories require accepting a license. After accepting it on the model page,
export a [Hugging Face access token](https://huggingface.co/settings/tokens) as `HF_TOKEN` in the
environment before starting your game:

```sh
export HF_TOKEN=hf_...
```

## Inspecting the model cache

The cache mirrors the Hugging Face repo layout, so you can look around it with any file browser:

```
~/.cache/nobodywho/models/
└── NobodyWho/
    └── Qwen_Qwen3-0.6B-GGUF/
        └── Qwen_Qwen3-0.6B-Q4_K_M.gguf
```

Deleting a folder makes NobodyWho re-download it on next use.
