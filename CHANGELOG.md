# Changelog

Notable user-facing changes to NobodyWho.

We follow [Semantic Versioning](https://semver.org/) for published bindings, which are released independently. Release entries list the package versions that contain the change.

Format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Pocket TTS speech synthesis, including Hugging Face authentication for gated model files. Available for all bindings.
- Automatic model selection: pass `"auto"` as a model path to select a recommended model based on available memory. Available for all bindings
- MTP support for attention models with separate MTP files. This is mainly Gemma 4. Available for all bindings.

### Changed

- **React Native:** `STT` now takes a named options object. Replace `new STT(source, language, quantization)` with `new STT({ source, language, quantization })`.

## [Python v1.6.0, Flutter v2.4.0, Godot v9.5.0, Kotlin v2.1.0, React Native v2.4.0, Swift v2.2.0] - 2026-07-13

### Added

- Offline text-to-speech through `Tts`, with Kokoro and Supertonic ONNX backends.
- Offline speech-to-text through `Stt`, with Whisper ONNX models.
- `Chat.getStats()` exposes context-window usage, while `Model.maxCtx()` returns the model's maximum context size.
- Tokenize messages and prompts without inferencing.
- Build prompts from JSON-serializable data.

### Fixed

- **Flutter:** Android builds are now compatible with Gradle 9.
- **Flutter:** Function-parsing failures provide clearer errors.
