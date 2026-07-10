---
title: Speech to Text
description: Transcribe spoken audio to text with NobodyWho in Flutter.
sidebar_position: 5
---

To transcribe audio into text, NobodyWho provides an integration with the Whisper models in ONNX format.

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;

// ... after NobodyWho.init().
final stt = nobodywho.Stt(source: 'hf://onnx-community/whisper-base');

final text = await stt.transcribeFile('recording.mp3').completed();
print(text);
```

If the audio is not coming from a file, but instead directly from a buffer, `transcribePcm` is available:

```dart continuation
final text = await stt.transcribePcm(samples, 16000).completed();
```

In order to make this work, the buffer needs to be mono i16 PCM samples. The sample rate can be anything - NobodyWho resamples internally to what Whisper expects.

As with classic Chat models, streaming is available, so the transcription can be consumed token by token:

```dart continuation
await for (final piece in stt.transcribeFile('recording.mp3')) {
  stdout.write(piece);
}
```

## Supported models

NobodyWho only supports Whisper models in **ONNX** format. `source` is a Hugging Face repo (`hf://owner/repo`) or a local directory containing such a model, e.g. `hf://onnx-community/whisper-base`. Browse the [Whisper ONNX models on Hugging Face](https://huggingface.co/models?library=onnx&search=whisper) to pick a size that fits your accuracy and speed needs.

You can also pick a `quantization` variant of the model to download and load. Lower-precision variants are smaller and faster, but can lose some transcription accuracy. Supported values are `default`, `fp16`, `int8`, `uint8`, `bnb4`, `q4`, `q4f16`, and `quantized`. Defaults to `default`.

```dart
final stt = nobodywho.Stt(
  source: 'hf://onnx-community/whisper-base',
  quantization: 'q4',
);
```

## Improving performance

By default, Whisper auto-detects the spoken language, which costs a bit of extra processing. If you already know the language, pass its ISO 639-1 code as `language` to skip detection and improve performance:

```dart
final stt = nobodywho.Stt(
  source: 'hf://onnx-community/whisper-base',
  language: 'en',
);
```
