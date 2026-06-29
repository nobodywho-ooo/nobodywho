# TTS

Text-to-speech via [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) — a lightweight model that produces 24 kHz audio.

## Quick start

```rust
use nobodywho::tts::{Tts, TtsConfig};

let tts = Tts::new(TtsConfig::kokoro("NobodyWho/Kokoro-82M"))?;
let wav: Vec<u8> = tts.synthesize("Hello from NobodyWho!")?;
std::fs::write("out.wav", wav)?;
```

The model is downloaded from HuggingFace on first use and cached locally. Pass a local directory path instead of a repo ID to skip the download.

## Voices and languages

Set `voice` and `language` together — they must agree.

| Language | `language` | Example voices |
|----------|------------|----------------|
| American English | `en-us` | `af_heart`, `am_michael` |
| British English | `en-gb` | `bf_emma`, `bm_george` |
| Spanish | `es` | `ef_dora`, `em_alex` |
| French | `fr` | `ff_siwis` |
| Italian | `it` | `if_sara`, `im_nicola` |
| Brazilian Portuguese | `pt-br` | `pf_dora`, `pm_alex` |

Japanese (`ja`) and Chinese (`zh`) voices are not supported.

```rust
use nobodywho::tts::{KokoroConfig, Tts, TtsConfig};

let mut cfg = KokoroConfig::new("NobodyWho/Kokoro-82M");
cfg.voice = "ff_siwis".into();
cfg.language = "fr".into();
cfg.speed = 1.0;

let tts = Tts::new(TtsConfig::Kokoro(cfg))?;
let wav = tts.synthesize("Bonjour le monde.")?;
```

## Async

`synthesize_async` is available for async contexts:

```rust
let wav = tts.synthesize_async("Hello!").await?;
```

`Tts` is `Clone` — the underlying worker thread is shared across clones.

## Hardware

`Tts::new` uses `TtsDevice::Auto` (CUDA if available, CPU otherwise).
Use `Tts::with_device` to be explicit:

```rust
use nobodywho::tts::TtsDevice;
let tts = Tts::with_device(config, TtsDevice::Cpu)?;
```
