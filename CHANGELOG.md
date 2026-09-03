# Changelog

Notable user-facing changes to NobodyWho.

We follow [Semantic Versioning](https://semver.org/) for published bindings, which are released independently. Release entries list the package versions that contain the change.

Format is based on [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `Chat.complete(messages)` answers a whole conversation passed as a list of messages, for when you would rather hand over the conversation than let the `Chat` remember it. The list becomes the chat history and the response is appended, so `ask()` continues from there. A system message at the front sets the chat's system prompt; leave it out and the prompt already on the chat is kept. Media referenced by the messages is re-read from its file path, so a saved conversation containing images or audio can be replayed. Available for all bindings.
- Message content can now be a list of typed parts, interleaving text with images and audio in a single message — the shape the OpenAI and Anthropic libraries use, so a multimodal conversation can be handed to `complete()` directly. Parts are `text`, `image` and `audio`; a plain string stays valid wherever content is accepted. Available for all bindings.
- `complete()` now accepts the chat's other settings per call — the sampler, the template variables and the tools. They follow the same rule as the system message: what you pass stays set, what you leave out is kept, so specifying all of them makes the call independent of whatever the chat is currently holding. Applying them in the same call as the turn also makes it atomic, where a setter followed by `complete()` is two round-trips another caller can interleave with. Note that changing the tools re-selects the chat template, so that turn re-prefills from near token zero. Python and Flutter take these as named arguments; Kotlin, Swift and React Native take an `Options` object; Godot takes a `NobodyWhoChatOptions` through a separate `complete_with_options()`, and omits tools since Godot registers those on the chat node with `add_tool()`. Available for all bindings.

### Changed

- **Breaking:** a message's media is now part of its content instead of a separate `assets` list, and the `Asset` type is gone. Where you passed `{"role": "user", "content": "...", "assets": [...]}`, pass a content list of parts instead. The media file path now lives on the part it belongs to, so the ordering of text and media within a message is explicit rather than implied. Affects all bindings.
- **Breaking:** the system prompt is no longer stored as the first chat message; it is a setting on the `Chat`, as in the Anthropic and Gemini SDKs. `get_chat_history()` therefore never returns a system message, and `complete()` no longer clears the system prompt when the list you pass has none. Media in a system message is now rejected, since no chat template supports it. Affects all bindings.
- **Breaking:** `PromptPart` is now `ContentPart`, since one type now describes both a prompt and a message's content, and its text variant's field is `text` rather than `content`. Only Swift is affected: it re-exports the type directly, so `[PromptPart]` becomes `[ContentPart]` and `.text(content:)` becomes `.text(text:)`. Flutter, Kotlin and React Native build prompts through their own part types, which are unchanged.
- **Breaking:** the errors Flutter's chat setters throw are no longer `SetterError`; catch them as plain exceptions rather than by type. This covers `setChatHistory`, `setSamplerConfig`, `setTools`, `setSystemPrompt`, `setTemplateVariable(s)`, `resetContext` and `resetHistory`. `SetterError` was an opaque Dart class carrying no message, so the reason a setter failed could not be read; the exception now carries the rendered error text, as the generation methods already did. The history validation `setChatHistory` performs is the same on every binding — see the system prompt entry above.
- **Godot:** a chat setter the worker rejects now emits `worker_failed` alongside logging the error, the same way a dropped generation is reported. `set_sampler_preset_*` and `set_sampler_config` previously discarded the error entirely.
- **Breaking:** raw JSON content now round-trips through a `{"type": "raw", "value": …}` wrapper. `text`, `image` and `audio` are reserved tags: a content array whose entries all carry one of them is read as content parts, while a non-empty array carrying none of them reaches the chat template as a real list — which is what `from_json()` is for, on models finetuned on structured turns. Since both shapes are arrays of `type`-tagged objects, raw content that happens to look like parts would otherwise be read back as parts, so `get_chat_history()` returns it wrapped; the wrapper is accepted on the way in and stripped before the template sees it. Mixing part tags with other tags in one array, or using a reserved tag with fields that do not parse, is now an error instead of being passed through untouched. Affects all bindings.
- Handing `complete()` both a sampler and a set of tools no longer compiles the tool-calling grammar twice for that turn, and no longer redoes the ~400 ms llguidance initialisation — nor does changing the sampler alone. The tokenizer state a grammar is compiled against depends on the model rather than the grammar, so it is now built once per chat and reused, leaving only the grammar compile itself and turning hundreds of milliseconds of per-turn overhead into single-digit milliseconds. Available for all bindings.

### Fixed

- A rejected chat setter no longer kills the chat. `set_sampler_config`, `set_tools` and `reset_chat` used to end the worker, so the reason was only logged and every later call — including `ask()` — failed with "worker terminated". The error now reaches the caller and the chat keeps working. Available for all bindings.
- A rejected encoder or cross-encoder input no longer kills the worker. Text longer than the context window used to end it, so every later `encode()` or `rank()` failed too. The error now reaches the caller and the worker stays usable. Available for all bindings.
- **React Native:** Logs are now visible in Xcode on iOS.
- **Swift:** Logs are now visible in Xcode on iOS.

### Removed
- **Flutter:** Removed `ToolCallExtension` and `ToolCall.argumentsJson` as `ToolCall` is no longer opaque.

## [Python v2.0.0, Flutter v3.0.0, Godot v10.0.0, Kotlin v3.0.0, React Native v3.0.0, Swift v3.0.0] - 2026-08-20

### Added

- Added `dynamic_temperature`, `top_n_sigma` and `logit_bias` sampler steps.
- Batch embedding through `Encoder.encode_batch()`. Available for all bindings.
- Added `VoiceActivityDetection` for detecting when audio includes speech. Available for all bindings.
- **Kotlin:** the coroutines API is now exposed for library consumers.

### Changed

- `CrossEncoder.rank()` now batches documents internally making re-raking faster. Affects all bindings.
- Tool calling now uses Lark grammars with the llguidance sampler instead of GBNF, making tool-constrained generation noticeably faster — especially on large-vocabulary models — and pre-builds the tool-call sampler so the first tool-enabled response no longer stalls while the grammar compiles. No API changes. Available for all bindings.
- **Behavior change:** creating a chat with tools on a model whose tool-call format cannot be detected now fails at setup instead of silently falling back to unconstrained (unreliable) tool calling. Chats created without tools are unaffected. Available for all bindings.
- **Breaking:** `STT` renamed to `SpeechToText`, `Tts` renamed to `TextToSpeech`. Affects all bindings.

### Fixed

- Context shifting now measures the shortened history, avoiding unnecessary history deletion and repeated tokenization. Affects all bindings.
- Reworked allocation handling during chat inference, reducing allocation calls by 62% and allocated bytes by 91%.
- Contructor for STT was synchronous, replaced with `load` function to make async and keep conventions
- Prefix caching now works on token-level complete prefixes, speeding up prefill. Affects all bindings.
- Vision-language models using M-RoPe positional embeddings no longer desynchronize the prefix cache when re-encoding images. Affects all bindings.
- Available-memory detection no longer fails when the cgroup v2 root has no memory limit. Affects all bindings.
- **Android:** the C++ runtime is now linked statically, so no companion `libc++_shared.so` has to be shipped and no NDK is needed to build against the library. Affects all Android artifacts.
- **Flutter:** Android builds now package the libc and onnxruntime `.so` files they depend on.
- **Swift:** visionOS and watchOS builds fixed.

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
