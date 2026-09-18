---
title: Getting started
description: How to setup NobodyWho in Godot
sidebar_position: 0
---

NobodyWho lets you run large language models **inside your Godot game**, entirely offline. Chat with
NPCs, transcribe player speech, generate voice lines, and embed text for search — all on the
player's own machine, with no servers, no API keys, and no telemetry.

## How do I get started?

NobodyWho is a GDExtension. Install it by copying the `addons/nobodywho` folder from a
[release](https://github.com/nobodywho-ooo/nobodywho/releases) into your project:

```
your-project/
└── addons/
    └── nobodywho/
        ├── nobodywho.gdextension
        ├── libnobodywho-godot-x86_64-unknown-linux-gnu-release.so
        └── ...
```

The release zip bundles the libraries for all supported platforms (Windows, Linux, macOS, and
Android), so the same folder works everywhere. Restart the editor after copying it in, and the
NobodyWho classes become available to GDScript.

:::info
NobodyWho requires **Godot 4.6 or newer**.
:::

With the extension installed, your first conversation is three lines:

```gdscript
func _ready():
    var chat = await NobodyWhoChat.create("hf://NobodyWho/Qwen_Qwen3-0.6B-GGUF", {})
    var stream = chat.ask("Hello! Who are you?")
    print(await stream.completed())
```

The model downloads to a local cache on first use — after that it works completely offline.
See [Downloading models](./downloading-models) for the supported path formats, and
[Chat](./chat) for the full chat walkthrough.

:::warning
Every async method resolves to the result, or `null` on failure (with details in the editor's
Errors tab). **Always await the returned value immediately** — storing it and awaiting it later is
unsupported and may hang.
:::

## Logging

The library logs through Godot's console (`godot_print`/`godot_warn`/`godot_error`). By default it
logs at `INFO` level; make it chattier while debugging with:

```gdscript
NobodyWho.set_log_level("DEBUG")
```

Valid levels are `"TRACE"`, `"DEBUG"`, `"INFO"`, `"WARN"`, and `"ERROR"`.

## What's available?

- [Chat](./chat) — talk to local LLMs, with full control over the context.
- [Tool calling](./tool-calling) — let the model call your GDScript functions.
- [Multimodal models](./vision) — image and audio input.
- [Sampling](./sampling) — tune token selection and constrain output to a format.
- [Speech to text](./speech-to-text) — Whisper transcription from files or microphone.
- [Text to speech](./text-to-speech) — generate voice lines as WAV bytes.
- [Voice activity detection](./voice-activity-detection) — know when the player starts and stops speaking.
- [Embeddings & RAG](./embeddings-and-rag) — semantic search over your game's text.

## Minimum recommended specs

- CPU: 4 cores
- RAM: 8 GB
- GPU: optional (Vulkan) — speeds up all models significantly

Smaller models (around 0.5-1B parameters, quantized) run comfortably on these specs, including on
 phones. Every feature works without a GPU, just slower.

## Feedback & Contributions

Bugs and feature requests go to our [issues](https://github.com/nobodywho-ooo/nobodywho/issues).
Want to chat or need help? Join us on [Discord](https://discord.gg/qhaMc2qCYB). Contributions are
welcome — see [CONTRIBUTING](https://github.com/nobodywho-ooo/nobodywho/blob/main/CONTRIBUTING.md)
for how to build from source.
