# TextToSpeech

Text-to-speech via [Kokoro](https://huggingface.co/hexgrad/Kokoro-82M) and [Supertonic](https://huggingface.co/Supertone/supertonic-3) ONNX architectures.

## Quick start

```rust
use nobodywho::text_to_speech::{TextToSpeech, TextToSpeechConfig};

let tts = TextToSpeech::new(TextToSpeechConfig::kokoro("hf://NobodyWho/Kokoro-82M"))?;
let wav: Vec<u8> = tts.synthesize("Hello from NobodyWho!")?;
std::fs::write("out.wav", wav)?;
```

The model is downloaded from HuggingFace on first use and cached locally. Pass a local directory path instead of an `hf://` repo to skip the download.

## Pocket TTS

[KevinAHM/pocket-tts-onnx](https://huggingface.co/KevinAHM/pocket-tts-onnx) is supported with its published language bundles and built-in voices. It defaults to CPU-friendly INT8 models and 24 kHz WAV output.

Kyutai keeps the built-in voice-state files in its gated `kyutai/pocket-tts` repository. Browse its [voice catalogue](https://github.com/kyutai-labs/pocket-tts?tab=readme-ov-file#voices), accept its terms, and set `HF_TOKEN`, or pass a token explicitly in the config (which takes precedence):

```rust
use nobodywho::text_to_speech::{PocketTtsConfig, TextToSpeech, TextToSpeechConfig};

let mut cfg = PocketTtsConfig::new("hf://KevinAHM/pocket-tts-onnx");
cfg.language = "english_2026-04".into();
cfg.voice = "alba".into();
cfg.huggingface_token = Some("hf_...".into());
let tts = TextToSpeech::new(TextToSpeechConfig::PocketTts(cfg))?;
```

Set `precision` to `PocketTtsPrecision::Fp32` to use full-precision ONNX weights. `temperature` controls generation variation and `lsd_steps` trades speed for flow-matching quality.

## Supertonic

Supertonic models use a directory with `onnx/` model files and `voice_styles/` JSON files. Passing an unknown voice returns a `MissingVoice` error listing the voices present in the model dir.

The upstream [Supertone/supertonic-3](https://huggingface.co/Supertone/supertonic-3) repo ships voices `F1`–`F5` and `M1`–`M5`.

```rust
use nobodywho::text_to_speech::{SupertonicConfig, TextToSpeech, TextToSpeechConfig};

let mut cfg = SupertonicConfig::new("hf://Supertone/supertonic-3");
cfg.language = "en".into();
cfg.voice = "M1".into();

let tts = TextToSpeech::new(TextToSpeechConfig::Supertonic(cfg))?;
let wav = tts.synthesize("Hello from NobodyWho!")?;
```

## Kokoro voices and languages

For Kokoro, set `voice` and `language` together — they must agree.

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
use nobodywho::text_to_speech::{KokoroConfig, TextToSpeech, TextToSpeechConfig};

let mut cfg = KokoroConfig::new("hf://NobodyWho/Kokoro-82M");
cfg.voice = "ff_siwis".into();
cfg.language = "fr".into();
cfg.speed = 1.0;

let tts = TextToSpeech::new(TextToSpeechConfig::Kokoro(cfg))?;
let wav = tts.synthesize("Bonjour le monde.")?;
```

## Async

`synthesize_async` is available for async contexts:

```rust
let wav = tts.synthesize_async("Hello!").await?;
```

`TextToSpeech` is `Clone` — the underlying worker thread is shared across clones.

## Hardware

`TextToSpeech::new` uses `TextToSpeechDevice::Auto` (CUDA if available, CPU otherwise).
Use `TextToSpeech::with_device` to be explicit:

```rust
use nobodywho::text_to_speech::TextToSpeechDevice;
let tts = TextToSpeech::with_device(config, TextToSpeechDevice::Cpu)?;
```
