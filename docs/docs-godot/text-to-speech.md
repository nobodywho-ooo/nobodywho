---
title: Text-to-speech
description: Generate WAV audio from text with NobodyWho in Godot.
sidebar_position: 8
---

NobodyWho can generate audio from any piece of text, in a wide variety of languages. You pass in text and get WAV bytes back, ready to save or play in your app. This process is also known as Text-to-Speech (or TTS).

## Quick start

Here's how you get started. Add a `NobodyWhoTts` node to your scene, then use it from a script:

```gdscript
extends Node

@onready var tts: NobodyWhoTts = $NobodyWhoTts

func _ready():
    tts.source = "NobodyWho/Kokoro-82M" # Hugging Face repo ID or local folder with the model files.
    tts.voice = "bf_emma" # Voice to use from the model.
    tts.language = "en-gb" # Language code for the input text.

    tts.start_worker()
    await tts.worker_started

    # Generate WAV bytes for this sentence.
    var result: Dictionary = await tts.synthesize("Hello from NobodyWho!")
    if not result.ok:
        push_error(result.error)
        return

    # Save the audio to a file.
    var file = FileAccess.open("user://out.wav", FileAccess.WRITE)
    file.store_buffer(result.wav)
```

Let’s start with `source`: it tells NobodyWho which TTS model to load. More on that in the next section.

## Models and sources

NobodyWho currently supports two main model sources:

- `NobodyWho/Kokoro-82M`: Kokoro, a lightweight 24 kHz speech synthesis model. See the [Kokoro project](https://github.com/hexgrad/kokoro) and [model page](https://huggingface.co/hexgrad/Kokoro-82M).
- `Supertone/supertonic-3`: Supertonic, a multi-stage ONNX speech synthesis model with voice styles. See the [Supertonic project](https://github.com/supertone-inc/supertonic) and [model page](https://huggingface.co/Supertone/supertonic-3).

## Kokoro

For Kokoro, set `voice` and `language` together. They must agree with the model's available voices.

```gdscript
tts.source = "NobodyWho/Kokoro-82M"
tts.voice = "bf_emma"
tts.language = "en-gb"
```

Optional settings include:

- `voice`: voice to use from the model, e.g. `bf_emma`. See the [Kokoro voices folder](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main/voices) for the full list. Defaults to `bf_emma`.
- `language`: input language code. Supported values are listed on the [Kokoro model page](https://huggingface.co/NobodyWho/Kokoro-82M). Defaults to `en-gb`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Set `0` to use the backend default.

## Supertonic

For Supertonic, you can start with the default `voice` and `language`, or set them explicitly.

```gdscript
tts.source = "Supertone/supertonic-3"
tts.language = "en"
```

Optional settings include:

- `voice`: voice style. Supported values are `M1` to `M5` and `F1` to `F5`. Defaults to `M1`.
- `language`: input language code. See the [Supertonic model page](https://huggingface.co/Supertone/supertonic-3#supported-languages) for the full list. Defaults to `en`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Set `0` to use the backend default.
- `steps`: denoising steps. Higher values can improve quality but are slower. Lower values are faster but can sound rougher. Set `0` to use the backend default.
- `silence_duration`: seconds of silence between long text chunks. Higher values add longer pauses. Set `-1` to use the backend default.

## Backend

`backend` is the TTS engine/model family behind a source. In most cases, you do not need to set it because NobodyWho can infer it from `source`.

Set `backend` when you use a local directory, Godot path, or a custom source that NobodyWho cannot recognize:

```gdscript
tts.source = "res://models/kokoro-folder"
tts.backend = "kokoro"
```

Supported backend values are `kokoro` and `supertonic`.

## GPU

TTS uses GPU acceleration by default when available. Disable it with `device = "cpu"`:

```gdscript
tts.source = "Supertone/supertonic-3"
tts.device = "cpu"
tts.start_worker()
```

## Local model folder format

When `source` is a local directory or Godot path, point it at the top-level model folder and set the matching `backend`.

Use the Hugging Face file browsers as the reference layouts:

- Kokoro: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main)
- Supertonic: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3/tree/main)

For Supertonic, that top-level folder must include both the `onnx/` and `voice_styles/` directories. Download the model files with the same relative paths, then pass that folder as `source`.
