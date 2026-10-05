---
title: Text to Speech
description: Generate WAV audio from text with NobodyWho in C#.
sidebar_position: 6
---

Generate natural-sounding speech from text, ready to save as a WAV file or play back in your app.

```csharp
using NobodyWho;

using var tts = await TextToSpeech.LoadAsync(
    "hf://NobodyWho/Kokoro-82M", // Hugging Face repo (hf://owner/repo) or local folder with the model files.
    voice: "bf_emma",            // Voice to use from the model.
    language: "en-gb");          // Language code for the input text.

// Generate WAV bytes for this sentence.
byte[] wav = await tts.SynthesizeAsync("Hello from NobodyWho!");
// Save the audio to a file.
await File.WriteAllBytesAsync("out.wav", wav);
```

`LoadAsync` and `SynthesizeAsync` run on a background thread. If you are already off the UI thread, the synchronous `new TextToSpeech(...)` constructor (same parameters) and `Synthesize(text)` do the same work on the calling thread.

## Models and sources

NobodyWho supports three speech synthesis architectures, all in ONNX format:

- [Kokoro](https://github.com/hexgrad/kokoro), a lightweight 24 kHz speech synthesis model. Model page: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M).
- [Pocket TTS](https://github.com/kyutai-labs/pocket-tts), a compact 24 kHz speech synthesis model. Model page: [`KevinAHM/pocket-tts-onnx`](https://huggingface.co/KevinAHM/pocket-tts-onnx).
- [Supertonic](https://github.com/supertone-inc/supertonic), a multi-stage speech synthesis model with voice styles. Model page: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3).

`source` can be a Hugging Face repo (`hf://owner/repo`) as shown above, or a local directory laid out the same way as that repo. See [Local model folder format](#local-model-folder-format) and [Architecture](#architecture) for setup details.

## Kokoro

For Kokoro, set `voice` and `language` together. They must agree with the model's available voices.

```csharp
using var tts = await TextToSpeech.LoadAsync(
    "hf://NobodyWho/Kokoro-82M",
    voice: "bf_emma",
    language: "en-gb");
```

Optional settings include:

- `voice`: voice to use from the model, e.g. `bf_emma`. See the [Kokoro voices folder](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main/voices) for the full list. Defaults to `bf_emma`.
- `language`: input language code. Supported values are listed on the [Kokoro model page](https://huggingface.co/NobodyWho/Kokoro-82M). Defaults to `en-gb`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Defaults to `1.0`.

## Supertonic

For Supertonic, you can start with the default `voice` and `language`, or set them explicitly.

```csharp
using var tts = await TextToSpeech.LoadAsync(
    "hf://Supertone/supertonic-3",
    language: "en");
```

Optional settings include:

- `voice`: voice style. Supported values are `M1` to `M5` and `F1` to `F5`. Defaults to `M1`.
- `language`: input language code. See the [Supertonic model page](https://huggingface.co/Supertone/supertonic-3#supported-languages) for the full list. Defaults to `en`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Defaults to `1.05`.
- `steps`: denoising steps. Higher values can improve quality but are slower. Lower values are faster but can sound rougher. Must be greater than `0`; defaults to `8`.
- `silenceDuration`: seconds of silence between long text chunks. Higher values add longer pauses. Must be `0` or higher; defaults to `0.3`.

## Pocket TTS

For Pocket TTS, start with the default voice and language, or set them explicitly.

```csharp
using var tts = await TextToSpeech.LoadAsync(
    "hf://KevinAHM/pocket-tts-onnx",
    voice: "alba",
    language: "english_2026-04",
    huggingFaceToken: "hf_..."); // Alternative: set the HF_TOKEN environment variable.
```

Optional settings include:

- `voice`: built-in voice name. See the [Pocket TTS voice catalogue](https://github.com/kyutai-labs/pocket-tts?tab=readme-ov-file#voices). Defaults to `alba`.
- `language`: language bundle name. See the [available language bundles](https://huggingface.co/KevinAHM/pocket-tts-onnx/tree/main/onnx). Defaults to `english_2026-04`.
- `steps`: quality steps. Higher values are slower. Defaults to `1`.
- `precision`: `int8` for faster loading or `fp32` for higher quality. Defaults to `int8`.
- `temperature`: controls how varied the speech sounds. Defaults to `0.7`.

Pocket TTS voice states are gated in [`kyutai/pocket-tts`](https://huggingface.co/kyutai/pocket-tts). Accept its terms, then pass a [Hugging Face access token](https://huggingface.co/settings/tokens) with `huggingFaceToken`, or alternatively set the `HF_TOKEN` environment variable.

## Architecture

`architecture` is the TextToSpeech model family behind a source. In most cases, you do not need to set it because NobodyWho can infer it by looking for "kokoro", "pocket-tts", or "supertonic" in the `source` string.

Set `architecture` when you use a local directory or a custom source that NobodyWho cannot recognize:

```csharp
using var tts = await TextToSpeech.LoadAsync(
    "/path/to/local/kokoro-folder",
    architecture: TextToSpeechArchitecture.Kokoro);
```

Supported architecture values are `TextToSpeechArchitecture.Kokoro`, `TextToSpeechArchitecture.PocketTts` and `TextToSpeechArchitecture.Supertonic`.

## GPU

TextToSpeech uses GPU acceleration by default when available: the default `device`, `InferenceDevice.Auto`, runs on an NVIDIA GPU through CUDA if it can and on the CPU otherwise. Force the CPU with `device: InferenceDevice.Cpu`, or require CUDA with `InferenceDevice.Cuda`:

```csharp
using var tts = await TextToSpeech.LoadAsync(
    "hf://Supertone/supertonic-3",
    device: InferenceDevice.Cpu);
```

## Local model folder format

When `source` is a local directory, point it at the top-level model folder and pass the matching `architecture`.

Use the Hugging Face file browsers as the reference layouts:

- Kokoro: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main)
- Supertonic: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3/tree/main)

For Supertonic, that top-level folder must include both the `onnx/` and `voice_styles/` directories. Download the model files with the same relative paths, then pass that folder as `source`.
