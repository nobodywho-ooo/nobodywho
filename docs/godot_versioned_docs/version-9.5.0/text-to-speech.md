---
title: Text to Speech
description: Generate WAV audio from text with NobodyWho in Godot.
sidebar_position: 9
---

Generate natural-sounding speech from text, ready to save as a WAV file or play back in your app.

## Quick start

Add a `NobodyWhoTts` node to your scene, then use it from a script:

```gdscript
extends Node

@onready var tts: NobodyWhoTts = $NobodyWhoTts

func _ready():
    tts.source = "NobodyWho/Kokoro-82M"

    tts.start_worker()
    await tts.worker_started

    var result: Dictionary = await tts.synthesize("Hello from NobodyWho!")
    if not result.ok:
        push_error(result.error)
        return

    var file = FileAccess.open("user://out.wav", FileAccess.WRITE)
    file.store_buffer(result.wav)
```

## Models and sources

NobodyWho supports two speech synthesis backends, both in ONNX format:

- [Kokoro](https://github.com/hexgrad/kokoro), a lightweight 24 kHz speech synthesis model. Model page: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M).
- [Supertonic](https://github.com/supertone-inc/supertonic), a multi-stage speech synthesis model with voice styles. Model page: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3).

`source` can be a Hugging Face repo ID as shown above, a Godot path (`res://` or `user://`), or a local filesystem directory laid out the same way as that repo. For Supertonic, that directory must have both the `onnx/` and `voice_styles/` folders at its root.

If you point `source` at a local directory NobodyWho doesn't recognize, set `backend` explicitly to tell it which engine to use:

```gdscript
tts.source = "res://models/kokoro-folder"
tts.backend = "kokoro"
```

Supported `backend` values are `kokoro` and `supertonic`.

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
