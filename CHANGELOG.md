# Changelog

Notable user-facing changes to NobodyWho.

We follow [Semantic Versioning](https://semver.org/) for published bindings, which are released independently. Release entries list the package versions that contain the change.

Format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- Batch embedding through `Encoder.encode_batch()`. Available for all bindings.

### Changed

- `CrossEncoder.rank()` now batches documents internally making re-raking faster. Affects all bindings.
- Tool calling now uses Lark grammars with the llguidance sampler instead of GBNF, making tool-constrained generation noticeably faster — especially on large-vocabulary models — and pre-builds the tool-call sampler so the first tool-enabled response no longer stalls while the grammar compiles. No API changes. Available for all bindings.
- **Behavior change:** creating a chat with tools on a model whose tool-call format cannot be detected now fails at setup instead of silently falling back to unconstrained (unreliable) tool calling. Chats created without tools are unaffected. Available for all bindings.
- `STT` renamed to `SpeechToText`, `Tts` renamed to `TextToSpeech` 

### Fixed

- Context shifting now measures the shortened history, avoiding unnecessary history deletion and repeated tokenization. Affects all bindings.
- Reworked allocation handling during chat inference, reducing allocation calls by 62% and allocated bytes by 91%.
- Contructor for STT was synchronous, replaced with `load` function to make async and keep conventions

## [Python v1.7.0, Flutter v2.5.0, Godot v9.6.0, Kotlin v2.2.0, React Native v2.5.0, Swift v2.3.0] - 2026-07-30

### Added

- Chat now accepts a CPU thread count (`n_threads` / `threadCount` / `thread_count`), for leaving CPU headroom for other work. Defaults to the detected physical core count. Available for all bindings.
- Pocket TTS speech synthesis, including Hugging Face authentication for gated model files. Available for all bindings.
- Automatic model selection: pass `"auto"` as a model path to select a recommended model based on available memory. Available for all bindings
- MTP support for attention models with separate MTP files. This is mainly Gemma 4. Available for all bindings.

### Changed

- Inference now defaults to one thread per physical core (performance cores only, on Apple silicon) instead of one per logical CPU. Hyperthread siblings and efficiency cores pace the whole thread pool, so the old default was measurably slower — up to 2x on CPU-only generation. Affects all bindings.

### Fixed

- Grammar-constrained GBNF presets (`json` and the deprecated `grammar` preset) now apply the grammar before the truncation samplers. Previously, models whose top-k candidates contained no grammar-valid token (e.g. thinking models like Qwen3) silently crashed the process during generation. Affects all bindings.
- **Godot:** Windows debug builds now load the debug library. The `.gdextension` previously pointed the debug entry at the release DLL, so errors lacked stack traces.
- **Godot:** The distributable zip now includes the license file and stays under 1 GB (debug builds removed from the artifact) to meet the Godot Asset Store rules.

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
