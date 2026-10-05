---
title: Speech to Text
description: Transcribe spoken audio to text with NobodyWho in C#.
sidebar_position: 4
---

To transcribe audio into text, NobodyWho provides an integration with the Whisper models in ONNX format.

```csharp
using NobodyWho;

using var stt = await SpeechToText.LoadAsync("hf://onnx-community/whisper-base");

string text = await stt.TranscribeFile("recording.mp3").CompletedAsync();
Console.WriteLine(text);
```

`TranscribeFile` accepts WAV and MP3 files. If the audio is not coming from a file, but instead directly from a buffer, `TranscribePcm` is available:

```csharp
string text = await stt.TranscribePcm(samples, sampleRate: 16000).CompletedAsync();
```

In order to make this work, the buffer needs to be mono 16-bit PCM samples (`short[]`). The sample rate can be anything - NobodyWho resamples internally to what Whisper expects.

As with classic Chat models, streaming is available, so the transcription can be consumed token by token with `await foreach`:

```csharp
await foreach (var piece in stt.TranscribeFile("recording.mp3"))
{
    Console.Write(piece);
}
```

Passing a `CancellationToken` (to `CompletedAsync`, or with `WithCancellation` on the stream) stops waiting for the transcript; the transcription itself finishes in the background.

## Supported models

NobodyWho only supports Whisper models in **ONNX** format. `source` is a Hugging Face repo (`hf://owner/repo`) or a local directory containing such a model, e.g. `hf://onnx-community/whisper-base`. Browse the [Whisper ONNX models on Hugging Face](https://huggingface.co/models?library=onnx&search=whisper) to pick a size that fits your accuracy and speed needs.

You can also pick a `quantization` variant of the model to download and load. Lower-precision variants are smaller and faster, but can lose some transcription accuracy. Supported values are `default`, `fp16`, `int8`, `uint8`, `bnb4`, `q4`, `q4f16`, and `quantized`. Defaults to `default`.

```csharp
using var stt = await SpeechToText.LoadAsync(
    "hf://onnx-community/whisper-base",
    quantization: "q4");
```

## Improving performance

By default, Whisper auto-detects the spoken language, which costs a bit of extra processing. If you already know the language, pass its ISO 639-1 code as `language` to skip detection and improve performance:

```csharp
using var stt = await SpeechToText.LoadAsync(
    "hf://onnx-community/whisper-base",
    language: "en");
```
