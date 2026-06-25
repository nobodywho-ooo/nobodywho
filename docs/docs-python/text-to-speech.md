---
title: Text-to-speech
description: Generate audio from text with NobodyWho in Python.
sidebar_position: 8
---

NobodyWho can synthesize text to WAV bytes with the `Tts` class.

Two backends are available:

- `kokoro`: lightweight 24 kHz speech synthesis. This is the default when `backend` is not provided.
- `supertonic`: multi-stage ONNX speech synthesis with voice styles.

Models are downloaded from Hugging Face on first use and cached locally. You can also pass a local model directory.

## Kokoro

Read more about Kokoro on its [official Hugging Face page](https://huggingface.co/hexgrad/Kokoro-82M). NobodyWho currently supports Kokoro language codes `en-us`, `en-gb`, `es`, `fr`, `it`, and `pt-br`.

```python notest
from pathlib import Path
from nobodywho import Tts

tts = Tts(
    source="NobodyWho/Kokoro-82M",
    backend="kokoro",
    voice="bf_emma",
    language="en-gb",
)

wav = tts.synthesize("Hello from NobodyWho!")
Path("out.wav").write_bytes(wav)
```

For Kokoro, set `voice` and `language` together. They must agree with the model's available voices.

## Supertonic

Read more about Supertonic on its [official Hugging Face page](https://huggingface.co/Supertone/supertonic-3), including supported languages and voice styles.

```python notest
from pathlib import Path
from nobodywho import Tts

tts = Tts(
    source="Supertone/supertonic-3",
    backend="supertonic",
)

wav = tts.synthesize("Hello from NobodyWho!")
Path("out.wav").write_bytes(wav)
```

By default, Supertonic uses `voice="M1"` and `language="en"`. The upstream model includes voice styles `M1`–`M5` and `F1`–`F5`.

Most users can start with the defaults. Optional settings include:

- `voice`: voice style, e.g. `M1` or `F1`. Defaults to `M1`.
- `language`: input language code. Defaults to `en`.
- `speed`: speech speed multiplier. Values above `1.0` are faster. Defaults to `1.05`.
- `steps`: denoising steps. Higher can improve quality but is slower. Defaults to `8`.
- `silence_duration`: silence inserted between long text chunks. Defaults to `0.3` seconds.

## Async

```python notest
wav = await tts.synthesize_async("Hello!")
```

## Device selection

By default, NobodyWho uses `device="auto"`. You can also choose `cpu` or `cuda`:

```python notest
tts = Tts(
    source="Supertone/supertonic-3",
    backend="supertonic",
    device="cpu",
)
```
