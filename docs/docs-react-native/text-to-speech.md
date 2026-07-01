---
title: Text-to-speech
description: Generate WAV audio from text with NobodyWho in React Native.
sidebar_position: 6
---

NobodyWho can generate audio from any piece of text, in a wide variety of languages. You pass in text and get WAV bytes back, ready to save or play in your app. This process is also known as Text-to-Speech (or TTS).

## Quick start

Here's how you get started:

```typescript
import { Tts } from "react-native-nobodywho";

// Create a TTS model from a Hugging Face repo ID or local folder.
const tts = await Tts.load({
  source: "NobodyWho/Kokoro-82M",
  voice: "bf_emma",
  language: "en-gb",
});

// Generate WAV bytes for this sentence.
const wav = await tts.synthesize("Hello from NobodyWho!");
// wav is a Uint8Array containing WAV bytes.
```

That was a lot. No need to panic. Start with `source`: it tells NobodyWho which TTS model to load. More on that in the next section.

## Models and sources

NobodyWho currently supports two main model sources:

- `NobodyWho/Kokoro-82M`: Kokoro, a lightweight 24 kHz speech synthesis model. See the [Kokoro project](https://github.com/hexgrad/kokoro) and [model page](https://huggingface.co/hexgrad/Kokoro-82M).
- `Supertone/supertonic-3`: Supertonic, a multi-stage ONNX speech synthesis model with voice styles. See the [Supertonic project](https://github.com/supertone-inc/supertonic) and [model page](https://huggingface.co/Supertone/supertonic-3).

## Kokoro

For Kokoro, set `voice` and `language` together. They must agree with the model's available voices.

```typescript
const tts = await Tts.load({
  source: "NobodyWho/Kokoro-82M",
  voice: "bf_emma",
  language: "en-gb",
});
```

Optional settings include:

- `voice`: voice to use from the model, e.g. `bf_emma`. See the [Kokoro voices folder](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main/voices) for the full list. Defaults to `bf_emma`.
- `language`: input language code. Supported values are listed on the [Kokoro model page](https://huggingface.co/NobodyWho/Kokoro-82M). Defaults to `en-gb`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Defaults to `1.0`.

## Supertonic

For Supertonic, you can start with the default `voice` and `language`, or set them explicitly.

```typescript
const tts = await Tts.load({
  source: "Supertone/supertonic-3",
  language: "en",
});
```

Optional settings include:

- `voice`: voice style. Supported values are `M1` to `M5` and `F1` to `F5`. Defaults to `M1`.
- `language`: input language code. See the [Supertonic model page](https://huggingface.co/Supertone/supertonic-3#supported-languages) for the full list. Defaults to `en`.
- `speed`: speech speed multiplier. `1.0` is normal speed, lower values are slower, higher values are faster. Defaults to `1.05`.
- `steps`: denoising steps. Higher values can improve quality but are slower. Lower values are faster but can sound rougher. Must be greater than `0`; defaults to `8`.
- `silenceDuration`: seconds of silence between long text chunks. Higher values add longer pauses. Must be `0` or higher; defaults to `0.3`.

## Backend

`backend` is the TTS engine/model family behind a source. In most cases, you do not need to set it because NobodyWho can infer it from `source`.

Set `backend` when you use a local directory or a custom source that NobodyWho cannot recognize:

```typescript
const tts = await Tts.load({
  source: "/path/to/local/kokoro-folder",
  backend: "kokoro",
});
```

Supported backend values are `kokoro` and `supertonic`.

## GPU

TTS uses GPU acceleration by default when available. Disable it with `device: "cpu"`:

```typescript
const tts = await Tts.load({
  source: "Supertone/supertonic-3",
  device: "cpu",
});
```

## Local model folder format

When `source` is a local directory, point it at the top-level model folder and pass the matching `backend`.

Use the Hugging Face file browsers as the reference layouts:

- Kokoro: [`NobodyWho/Kokoro-82M`](https://huggingface.co/NobodyWho/Kokoro-82M/tree/main)
- Supertonic: [`Supertone/supertonic-3`](https://huggingface.co/Supertone/supertonic-3/tree/main)

For Supertonic, that top-level folder must include both the `onnx/` and `voice_styles/` directories. Download the model files with the same relative paths, then pass that folder as `source`.
