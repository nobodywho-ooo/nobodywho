---
title: Text to Speech
description: Generate WAV audio from text with NobodyWho in Flutter.
sidebar_position: 6
---

Generate natural-sounding speech from text, ready to save as a WAV file or play back in your app.

```dart
import 'dart:io';
import 'package:nobodywho/nobodywho.dart' as nobodywho;

// ... after NobodyWho.init().
final tts = await nobodywho.Tts.load(
  source: 'hf://NobodyWho/Kokoro-82M', // Hugging Face repo (hf://owner/repo) or local folder with the model files.
  voice: 'bf_emma', // Voice to use from the model.
  language: 'en-gb', // Language code for the input text.
);

// Generate WAV bytes for this sentence.
final wav = await tts.synthesize(text: 'Hello from NobodyWho!');
// Save the audio to a file.
await File('out.wav').writeAsBytes(wav);
```

## Models and sources

NobodyWho supports two speech synthesis architectures, both in ONNX format:

- [Kokoro](https://github.com/hexgrad/kokoro), a lightweight 24 kHz speech synthesis model. Model page: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M).
- [Supertonic](https://github.com/supertone-inc/supertonic), a multi-stage speech synthesis model with voice styles. Model page: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3).

`source` can be a Hugging Face repo (`hf://owner/repo`) as shown above, or a local directory laid out the same way as that repo. See [Local model folder format](#local-model-folder-format) and [Architecture](#architecture) for setup details.

## Kokoro

For Kokoro, set `voice` and `language` together. They must agree with the model's available voices.

```dart
// ... after NobodyWho.init().
final tts = await nobodywho.Tts.load(
  source: 'hf://NobodyWho/Kokoro-82M',
  voice: 'bf_emma',
  language: 'en-gb',
);
```

Optional settings include:

- `voice`: voice to use from the model, e.g. `bf_emma`. See the [Kokoro voices folder](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main/voices) for the full list. Defaults to `bf_emma`.
- `language`: input language code. Supported values are listed on the [Kokoro model page](https://huggingface.co/NobodyWho/Kokoro-82M). Defaults to `en-gb`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Defaults to `1.0`.

## Supertonic

For Supertonic, you can start with the default `voice` and `language`, or set them explicitly.

```dart
// ... after NobodyWho.init().
final tts = await nobodywho.Tts.load(
  source: 'hf://Supertone/supertonic-3',
  language: 'en',
);
```

Optional settings include:

- `voice`: voice style. Supported values are `M1` to `M5` and `F1` to `F5`. Defaults to `M1`.
- `language`: input language code. See the [Supertonic model page](https://huggingface.co/Supertone/supertonic-3#supported-languages) for the full list. Defaults to `en`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Defaults to `1.05`.
- `steps`: denoising steps. Higher values can improve quality but are slower. Lower values are faster but can sound rougher. Must be greater than `0`; defaults to `8`.
- `silenceDuration`: seconds of silence between long text chunks. Higher values add longer pauses. Must be `0` or higher; defaults to `0.3`.

## Architecture

`architecture` is the TTS model family behind a source. In most cases, you do not need to set it because NobodyWho can infer it by looking for "kokoro" or "supertonic" in the `source` string.

Set `architecture` when you use a local directory or a custom source that NobodyWho cannot recognize:

```dart
// ... after NobodyWho.init().
final tts = await nobodywho.Tts.load(
  source: '/path/to/local/kokoro-folder',
  architecture: .kokoro',
);
```

Supported architecture values are `kokoro` and `supertonic`.

## GPU

TTS uses GPU acceleration by default when available. Disable it with `useGpu: false`:

```dart
// ... after NobodyWho.init().
final tts = await nobodywho.Tts.load(
  source: 'hf://Supertone/supertonic-3',
  useGpu: false,
);
```

## Local model folder format

When `source` is a local directory, point it at the top-level model folder and pass the matching `architecture`.

Use the Hugging Face file browsers as the reference layouts:

- Kokoro: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main)
- Supertonic: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3/tree/main)

For Supertonic, that top-level folder must include both the `onnx/` and `voice_styles/` directories. Download the model files with the same relative paths, then pass that folder as `source`.
