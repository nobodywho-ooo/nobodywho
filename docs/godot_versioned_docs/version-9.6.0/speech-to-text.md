---
title: Speech to Text
description: Transcribe spoken audio to text with NobodyWho in Godot.
sidebar_position: 8
---

To transcribe audio into text, NobodyWho provides an integration with the Whisper models in ONNX format, through the `NobodyWhoSTT` node.

## Quick start

Add a `NobodyWhoSTT` node to your scene, then use it from a script:

```gdscript
extends Node

@onready var stt: NobodyWhoSTT = $NobodyWhoSTT

func _ready():
    stt.model_path = "hf://onnx-community/whisper-base"

    stt.start_worker()
    await stt.worker_started

    stt.transcribe_file("res://recording.mp3")
    var text = await stt.transcription_finished
    print(text)
```

If the audio is not coming from a file, but instead directly from a buffer, `transcribe_pcm` is available:

```gdscript
stt.transcribe_pcm(samples, 16000) # samples: PackedByteArray, sample_rate: int
var text = await stt.transcription_finished
```

In order to make this work, `samples` needs to be a `PackedByteArray` of mono, little-endian i16 PCM samples. The sample rate can be anything - NobodyWho resamples internally to what Whisper expects.

As with the Chat node, streaming is available: connect to `transcription_updated` to consume the transcription piece by piece as it's decoded, alongside `transcription_finished` for the full transcript once it's done:

```gdscript
extends Node

@onready var stt: NobodyWhoSTT = $NobodyWhoSTT

func _ready():
    stt.transcription_updated.connect(_on_transcription_updated)
    stt.transcription_finished.connect(_on_transcription_finished)

    stt.model_path = "hf://onnx-community/whisper-base"
    stt.start_worker()
    await stt.worker_started

    stt.transcribe_file("res://recording.mp3")

func _on_transcription_updated(piece: String):
    $Label.text += piece

func _on_transcription_finished(text: String):
    print("Done: ", text)
```

## Supported models

NobodyWho only supports Whisper models in **ONNX** format. `model_path` is a Hugging Face repo (`hf://owner/repo`) or a local directory containing such a model, e.g. `hf://onnx-community/whisper-base`. Browse the [Whisper ONNX models on Hugging Face](https://huggingface.co/models?library=onnx&search=whisper) to pick a size that fits your accuracy and speed needs.

You can also pick a `quantization` variant of the model to download and load. Lower-precision variants are smaller and faster, but can lose some transcription accuracy. Supported values are `default`, `fp16`, `int8`, `uint8`, `bnb4`, `q4`, `q4f16`, and `quantized`. Defaults to `default`.

```gdscript
stt.model_path = "hf://onnx-community/whisper-base"
stt.quantization = "q4"
```

## Improving performance

By default, Whisper auto-detects the spoken language, which costs a bit of extra processing. If you already know the language, set its ISO 639-1 code on `language` to skip detection and improve performance:

```gdscript
stt.model_path = "hf://onnx-community/whisper-base"
stt.language = "en"
```
