## 5.1.0

### Added

- Added support for the OpenCL and Vulkan backends on Android. This should give substantially higher performance on Android phones.
- Chats with recurrent and hybrid models, such as Qwen3.5, no longer re-read the whole conversation on most turns. The chat now saves the model state after each user message and rewinds to it, where it used to start over whenever an earlier answer was re-rendered differently, for example when thinking is dropped from the history.

  MTP speculative decoding now also works with recurrent and hybrid models, where it used to fail when a draft was rejected. For Qwen3.5, which keeps its MTP layers in the model file, pass the model file itself as the draft model; it is only loaded once.

### Fixed

- Chats no longer leave stray tokens in the model's context after a reply. The chat assumed the model had read the whole rendered reply, including template text after the end-of-turn token that was never decoded, so later turns could land one token off and keep stale text in context (seen with Gemma 3).
- Qwen3.5 chats with an image no longer keep stale text in the model's context. The same applies to the older Qwen2-VL, Qwen2.5-VL and Qwen3-VL, and to PaddleOCR-VL. These models give an image fewer context positions than tokens, which the chat didn't account for, so after an image the old end of the conversation was never removed and new text was added after it. The context-full check and the reported context usage now also count all of an image's tokens.
- Speech-to-text no longer mixes up languages when one `SpeechToText` transcribes several files, or audio longer than 30 seconds. Language detection ran on whatever the previous transcription had left behind, so audio could be detected as, and transcribed in, the previous language.

## 5.0.0

### Added

- Context shifting is configurable: how many turns to always keep at the start and end, the size to shrink to (a fraction of the context size or a number of tokens), or turning it off so a full context is an error. Pass `ContextShiftOptions` when creating a chat or later with `set_context_shift`. **Godot:** use the `"context_shift"` config key and `set_context_shift()` with a bool or Dictionary.
- Loaded models expose the identifier used to load them through a read-only `source` property.
- `SamplerBuilder` gained `constrain_with_json_schema`, `constrain_with_regex`, `constrain_with_grammar` and `json`, so a constraint can be combined with a temperature or a repetition penalty — the equivalent `SamplerPresets` each produce a finished sampler and cannot be layered.
- Support for the model scheme that [llama.app](https://llama.app) uses for its GGUF models which is `owner/repo:quantization`, where the repo name must end with `-GGUF`. Unlike llama.cpp, a quantization with no exact match in the repo is an error rather than a fallback to the repo's first model.

### Changed

- Context shifting now forgets the fewest turns needed to shrink the chat to half the context size. Previously it could forget up to twice as many.
- `SamplerPresets.dry()` now actually applies the DRY penalty. Its multiplier was 0.0, which llama.cpp reads as "disabled", so the preset was a no-op that sampled exactly like the default one. It is now 0.8, the value the preset's other numbers (base 1.75, allowed length 2) are tuned for. The preset leads with its DRY step, so the penalty sees the whole vocabulary rather than what survived truncation.
- **Breaking:** `SamplerConfig.from_json()` rejects a saved config with a grammar step inside `steps`, naming the step it could not read; move that entry into a `"grammar_steps"` list to load it.
- `SamplerPresets.json()` and the new `SamplerBuilder.json()` now constrain with the JSON schema `{"type":"object"}` through llguidance, so they take the same faster per-token path as the other `constrain_with_*` presets. Output is still a JSON object of any shape, as the old grammar's root was an object too. The new grammar is slightly more permissive at the edges: the old one allowed at most one newline plus 20 spaces of indentation per gap, and could not emit exponents like `1e10`.
- `penalty_last_n` no longer accepts `-1`, use a positive value instead (good defaults are 64 for penalties sampling and 1024 for DRY sampling).
- **Breaking:** Every `SamplerPresets` entry now builds on the default sampler (top-k 20, top-p 0.95, temperature 0.6) and changes one thing, instead of producing a chain holding only its own step: enabling a constraint no longer drops the truncation and temperature you would otherwise be sampling with, and `top_k`, `top_p` and `temperature` each override their counterpart and leave the rest alone. Constrained output is valid as before but less random within the constrained set. `greedy` is unaffected — it needs no shift steps.
- **Breaking:** `SamplerConfig` holds its constraints in a new `grammar_steps` list instead of mixing them into `steps`, and `json_schema`, `regex` and `lark` are now grammar steps rather than shift steps. The chain runs the grammar steps, then the shift steps, then the sample step, so a grammar cannot end up behind a truncation step that leaves it nothing valid to pick. Shift steps run in the order you add them, which matters for `dry`, `penalties` and `logit_bias`: they only reshuffle a shortlist if you chain them after a truncation step. Note that llama.cpp's own default chain leads with the penalties.
- A system message is now allowed after the start of the chat history. It stays in the history, for the chat template to render in place. On a model without a system role, where the system prompt is folded into the first user message and only a leading one can be, generating reports an error saying where to move the instruction.

### Fixed

- **Breaking:** ONNX Runtime, used for speech-to-text, text-to-speech and voice activity detection, is updated from 1.24 to 1.28. CUDA acceleration now needs a driver that supports CUDA 13, as CUDA 12 builds are no longer shipped. On platforms without CUDA support, requesting the `cuda` device now fails with "CUDA is not supported on this platform".
- Logs are now forwarded to Dart's `package:logging`. Configure it as described in their documentation.
- A FunctionGemma tool call whose argument value spans multiple lines is no longer dropped. The tool-call grammar lets a value contain newlines (a file body, a code snippet), but the extractor stopped at the first newline and discarded the whole call, so no tool ran. Multi-line values are now parsed.
- Kokoro speech synthesis no longer garbles contractions written with typographic apostrophes (`’`, `‘`, `´`, `` ` ``) or curly double quotes (`“ ”`), which word processors and phone autocorrect produce — `it’s` was spoken "it-ess", `don’t` "don-tee". They now fold to their ASCII counterparts before phonemization, as the supertonic backend already did.
- Fix logs from llama.cpp's multimodal backend not being sent to the platform's logging mechanism.
- Speech-to-text works with the `fp16` and `q4f16` Whisper quantizations. Before, both failed while the model was loading.
- Text a model writes before a tool call is now kept in the chat history. Previously whatever a model generated (and streamed) before the tool call was forgotten and not visible in `get_chat_history()`. It is now stored as content in the assistant message and is rendered next to the tool call. Note that the tool call is still stored in history as the function name and its arguments.

### Removed

- **Breaking:** The deprecated `SamplerPresets.grammar()` preset and `SamplerBuilder.grammar()` step are gone, along with the `{"type": "grammar"}` entry in a serialized config. Both have been deprecated since June 2026 in favour of `constrain_with_grammar()`, which accepts the same GBNF as well as Lark and takes the faster llguidance path — switch to it and drop the `root` argument, which was always `"root"` in practice. The one thing it cannot express is a lazy grammar: `trigger_on`, which let the model write freely until a marker before the grammar took effect, has no llguidance equivalent and is removed with no replacement. Godot's method was `set_sampler_preset_grammar()`.
- **Breaking:** The `lark_with_slices` sampler step is gone. Nothing constructed it, so the only way to have one is a hand-written sampler config, and `SamplerConfig.from_json()` now rejects a payload containing `{"type": "lark_with_slices"}`. Change it to `{"type": "lark"}` to keep the same grammar.

## 4.0.0

### Breaking changes

- A message's media is now part of its content instead of a separate `assets` list, and the `Asset` type is gone (#674). The media file path lives on the part it belongs to, so the ordering of text and media within a message is explicit rather than implied.
- The system prompt is no longer stored as the first chat message; it is a setting on the `Chat` (#674). `getChatHistory()` therefore never returns a system message, and `complete()` no longer clears the system prompt when the list you pass has none. Media in a system message is now rejected, since no chat template supports it.
- Raw JSON content now round-trips through a `{"type": "raw", "value": ...}` wrapper (#674). `text`, `image` and `audio` are reserved tags: a content array whose entries all carry one of them is read as content parts, while a non-empty array carrying none of them reaches the chat template as a real list. Mixing part tags with other tags in one array, or using a reserved tag with fields that do not parse, is now an error instead of being passed through untouched.
- The errors the chat setters throw are no longer `SetterError`; catch them as plain exceptions rather than by type (#701). This covers `setChatHistory`, `setSamplerConfig`, `setTools`, `setSystemPrompt`, `setTemplateVariable(s)`, `resetContext` and `resetHistory`. `SetterError` was an opaque Dart class carrying no message, so the reason a setter failed could not be read; the exception now carries the rendered error text, as the generation methods already did.
- Removed `ToolCallExtension` and `ToolCall.argumentsJson`, as `ToolCall` is no longer opaque (#697).

### Chat completion (#674)

`Chat.complete(messages)` answers a whole conversation passed as a list of messages, for when you would rather hand over the conversation than let the `Chat` remember it. The list becomes the chat history and the response is appended, so `ask()` continues from there. A system message at the front sets the chat's system prompt; leave it out and the prompt already on the chat is kept. Media referenced by the messages is re-read from its file path, so a saved conversation containing images or audio can be replayed.

Message content can now be a list of typed parts, interleaving text with images and audio in a single message — the shape the OpenAI and Anthropic libraries use, so a multimodal conversation can be handed to `complete()` directly. Parts are `text`, `image` and `audio`; a plain string stays valid wherever content is accepted.

`complete()` also accepts the chat's other settings per call as named arguments — the sampler, the template variables and the tools. They follow the same rule as the system message: what you pass stays set, what you leave out is kept, so specifying all of them makes the call independent of whatever the chat is currently holding. Applying them in the same call as the turn also makes it atomic. Note that changing the tools re-selects the chat template, so that turn re-prefills from near token zero.

### New sampler steps (#687)

Added `dynamicTemperature`, `topNSigma` and `logitBias` sampler steps.

### Faster per-turn tool calling (#705)

Handing `complete()` both a sampler and a set of tools no longer compiles the tool-calling grammar twice for that turn, and no longer redoes the ~400 ms llguidance initialisation — nor does changing the sampler alone. The tokenizer state a grammar is compiled against depends on the model rather than the grammar, so it is now built once per chat and reused, turning hundreds of milliseconds of per-turn overhead into single-digit milliseconds.

### Changed

- Updated `flutter_rust_bridge` to 2.13.0 (#697).
- Updated `llama-cpp-rs` (#671).

### Fixes

- **Chat setters (#701)** — a rejected chat setter no longer kills the chat. `setSamplerConfig`, `setTools` and `resetHistory` used to end the worker, so the reason was only logged and every later call — including `ask()` — failed with "worker terminated". The error now reaches the caller and the chat keeps working.
- **Encoder workers (#702)** — a rejected encoder or cross-encoder input no longer kills the worker. Text longer than the context window used to end it, so every later `encode()` or `rank()` failed too. The error now reaches the caller and the worker stays usable.
- **Android builds on Windows (#706)** — building the Android package from a Windows host now works.
- **`NOBODYWHO_FLUTTER_XCFRAMEWORK_PATH` (#710)** — the override is honoured again.

## 3.0.0

### Breaking changes

- `Stt` is renamed to `SpeechToText` and `Tts` is renamed to `TextToSpeech` (#661). `SpeechToText` is now constructed with `SpeechToText.load(...)` instead of a synchronous constructor (#660). Update imports and references.
- Creating a chat with tools on a model whose tool-call format cannot be detected now throws at setup instead of silently falling back to unconstrained (unreliable) tool calling (#638). Chats created without tools are unaffected.

### Voice Activity Detection (#612)

New `VoiceActivityDetection` class for detecting when an audio stream includes speech.

### Batch embedding (#654)

`Encoder.encodeBatch()` generates embeddings for many inputs in one call, and `CrossEncoder.rank()` now batches documents internally, making re-ranking faster.

### Faster tool calling (#638)

Tool-constrained generation now uses Lark grammars with the llguidance sampler instead of GBNF, making it noticeably faster — especially on large-vocabulary models — and pre-building the tool-call sampler so the first tool-enabled response no longer stalls while the grammar compiles.

### Fixes

- **Android packaging (#662)** — the libc and onnxruntime `.so` files are now packaged into the Android build, which was previously unusable without them.
- **libc++_shared linking (#682)** — the C++ runtime is now linked statically to the already-existing NDK version, so no companion `libc++_shared.so` has to be shipped and no NDK is needed to build against the plugin.
- **Context shifting (#667)** — now measures the shortened history, avoiding unnecessary history deletion and repeated tokenization.
- **Reduced allocations (#666)** — reworked allocation handling during chat inference, cutting allocation calls by 62% and allocated bytes by 91%.
- **Prefix caching (#657)** — fixed token-level complete-prefix caching, speeding up prefill.
- **M-RoPe vision-language models (#688)** — VL models that apply M-RoPe positional embeddings while encoding images no longer corrupt the KV cache and break prefix caching.
- **cgroup v2 memory limit (#672)** — handle a missing cgroup v2 root memory limit instead of mis-detecting available memory.

### Documentation

- Documented the Android `INTERNET` permission requirement (#681).

## 2.5.0

### Automatic model selection (#630)

Pass `"auto"` as the model path to pick a recommended model that fits available memory, instead of hardcoding a model name.

### Gemma 4 / MTP support (#636)

Added MTP (multi-token prediction) support for attention models that ship separate MTP files — primarily Gemma 4.

### Pocket TTS (#641)

Added a Pocket TTS speech-synthesis backend, including Hugging Face authentication for downloading gated model files.

### Configurable CPU thread count (#650)

`Chat` now accepts a `threadCount` parameter to set the number of CPU worker threads, leaving headroom for other work. Inference now defaults to one thread per **physical** core (performance cores only on Apple silicon) instead of one per logical CPU, which measurably speeds up CPU-only generation.

### Fixed

- **Grammar-constrained GBNF presets (#644)** — the `json` and deprecated `grammar` presets now apply the grammar before the truncation samplers. Previously, models whose top-k candidates contained no grammar-valid token (e.g. thinking models like Qwen3) silently crashed during generation.

## 2.4.0

### Text-to-speech (#537, #596, #601, #623)

Added a `Tts` class for offline speech synthesis, backed by ONNX. Two architectures are supported: **Kokoro** (`hf://hexgrad/Kokoro-82M`) and **Supertonic** (`hf://Supertone/supertonic-3`). Pass an `architecture` of `"kokoro"` or `"supertonic"` when the source name doesn't already contain it. Synthesis streams PCM samples you can play or save to a WAV file. The HuggingFace ONNX resolution API was reworked so quantization variants are selected explicitly per source.

### Speech-to-text (#579, #606, #607, #609, #616)

Added an `Stt` class for offline transcription with Whisper ONNX models (`hf://onnx-community/whisper-base`). Transcribe an audio file or raw PCM samples and iterate the recognized text token-by-token or read it out with `completed()`. Whisper quantization is now selectable (`"q4"` is the default), incomplete downloads are resumed, and the audio conversion pipeline was simplified.

### Token stats and `max_ctx` (#580)

`Chat.getStats()` now returns a `ChatStats` exposing the context window size and how much of it is currently used. `Model.maxCtx()` returns the maximum context size the model was trained with.

### Tokenize method (#583)

`Chat.tokenize(message)` / `Chat.tokenizeWithPrompt(parts)` return the token ids for a message, letting you count tokens against a model's context window without running inference.

### Prompt from JSON (#590)

`Prompt.fromJson(data)` builds a `Prompt` from a JSON-serializable object, handy for constructing prompts from structured data or stored conversations.

### Fixes

- **Gradle 9.0 compatible Android build (#627)** — the Android build script now injects the `ExecOperations` service instead of the `project.exec { }` call that was removed in Gradle 9.0, so the plugin builds cleanly on modern Gradle versions.
- **Clearer Dart function-parsing errors (#575)** — tool functions that fail to parse now produce more actionable error messages.

### Under the hood

- Bumped `llama-cpp-rs` / `llama.cpp` (#560, #605).
- Split inference logic out of `chat.rs` into a dedicated `inference` module (#588).

## 2.3.0

### LFM2 tool calling (#564)

Added support for the LiquidAI LFM2 model family's tool-calling format, so LFM2 models can now drive tool use.

### Reproducible sampling with `seed` (#562)

The sampler builder now exposes a `seed` parameter, giving you explicit, reproducible control over sampling randomness. Backed by an internal typestate refactor of the builder.

### List cached models with `getCachedModels()` (#508)

New function to list every cached `.gguf` model alongside its size on disk.

### Fixes

- **Render LFM2.5 chat templates (#563)** — LFM2.5 models previously failed to load because their chat templates use `{% generation %}` tags; these are now rewritten to a no-op so the templates render correctly.
- **No more crashes when clearing setters on an empty chat (#559)** — Removed context syncing from the setters, fixing crashes when setting the system prompt or tools on an empty chat history.

## 2.2.0

### Improved error messages (#532)

Clearer, more actionable errors for the three places users most often hit trouble: model loading, model downloading, and context shifting. Messages now point at the likely cause (bad path, network failure, OOM, context window exhausted) instead of surfacing raw lower-level errors.

## 2.1.0

### Grammar sampling revamp (#524)

Structured output generation has been rebuilt on top of the [llguidance](https://github.com/microsoft/llguidance) backend, replacing the previous GBNF-only pipeline. The new API is faster, supports richer grammar formats, and gives clearer errors when a constraint fails to compile.

**New `SamplerPresets` constructors** for constrained generation:

- `SamplerPresets.constrainWithJsonSchema(schema: ...)` — constrain output to a JSON Schema. Accepts either a `Map` (encoded for you) or a JSON string.
- `SamplerPresets.constrainWithRegex(pattern: ...)` — constrain output to a regular expression.
- `SamplerPresets.constrainWithGrammar(grammar: ...)` — constrain output to a context-free grammar. Accepts **both Lark and GBNF** strings; GBNF is converted internally, so existing grammars keep working.

Examples:

```dart
// Regex — force the model to answer with exactly "yes" or "no"
final yesNo = SamplerPresets.constrainWithRegex(pattern: r'yes|no');

// JSON Schema — always-valid JSON matching the schema
final person = SamplerPresets.constrainWithJsonSchema(schema: {
  'type': 'object',
  'properties': {
    'name': {'type': 'string'},
    'age':  {'type': 'integer'},
  },
});

// Lark CFG — context-free grammar (CSV-like)
final lark = SamplerPresets.constrainWithGrammar(grammar: """
  start: record (NEWLINE record)* NEWLINE?
  record: field ("," field)*
  field: /[^,"\\n\\r]+/
  NEWLINE: /\\r?\\n/
""");

// GBNF — same constructor also accepts GBNF strings
final gbnf = SamplerPresets.constrainWithGrammar(grammar: 'root ::= "yes" | "no"');
```

### Deprecations

- `SamplerPresets.json()` → use `SamplerPresets.constrainWithJsonSchema()` for schema-validated JSON.
- `SamplerPresets.grammar(grammar: ...)` → use `SamplerPresets.constrainWithGrammar()` (accepts both Lark and GBNF).
- `SamplerBuilder.grammar(...)` (the builder-style grammar step) is deprecated in favor of the preset constructors above.

The deprecated methods continue to work for this release, but will be removed in a future major version.

## 2.0.0

### Breaking Changes

- **Refactored `Message` enum** — The `Message` type has been restructured into four distinct variants: `Message.User`, `Message.Assistant`, `Message.System`, and `Message.Tool`. The previous `Message.Message`, `Message.ToolCalls`, and `Message.ToolResp` variants have been removed. Tool calls are now represented as an optional `toolCalls` field on `Message.Assistant` instead of a separate variant. Update call sites:
  ```dart
  // Before
  Message.message(role: Role.user, content: "Hello")
  Message.toolCalls(role: Role.assistant, content: "", toolCalls: [...])
  Message.toolResp(role: Role.tool, name: "get_weather", content: "22°C")

  // After
  Message.user(content: "Hello")
  Message.assistant(content: "Hi!")
  Message.assistant(content: "", toolCalls: [...])
  Message.tool(name: "get_weather", content: "22°C")
  Message.system(content: "You are helpful.")
  ```
- **Removed `Role` enum** — The `Role` enum is no longer needed since the role is now encoded in the `Message` variant itself.

## 1.2.0

### Features

- **Download progress callback** — Remote model loads (`hf://` and `https://`) now report progress via an `onDownloadProgress(downloaded, total)` callback so you can drive a progress UI during multi-GB downloads. (#498)

### Bug Fixes

- **Embeddings**: pooling type is now read from GGUF metadata, fixing incorrect embeddings for models that specify a non-default pooling type. (#500)
- **Embeddings**: explicitly mark all tokens as output during encoder runs, silencing a spurious llama.cpp warning. (Behavioral no-op — llama.cpp was already enabling outputs on all tokens for embeddings; this just suppresses the warning.) (#500)
- **GPU memory estimation**: account for the output/embedding layer when computing the GPU/CPU split. Previously the layer count was off by one, leaving layer 0 on CPU and forcing a CPU↔GPU round-trip per token — which could degrade inference speed by 3–30× depending on model size. (#504)

### Documentation

- Improved vision and audio (hearing) docs and examples. (#489)

## 1.1.0

- Add support for Qwen3.5 and Qwen3.6 tool calling

## 1.0.0

### Breaking Changes

- **Renamed `imageIngestion` to `projectionModelPath`** — The parameter on `Model.load()` and `Chat.fromPath()` has been renamed from `imageIngestion` to `projectionModelPath` to better reflect its purpose. Update call sites:
  ```dart
  // Before
  final model = Model.load("model.gguf", imageIngestion: "mmproj.gguf");
  final chat = await Chat.fromPath(modelPath: "model.gguf", imageIngestion: "mmproj.gguf");

  // After
  final model = Model.load("model.gguf", projectionModelPath: "mmproj.gguf");
  final chat = await Chat.fromPath(modelPath: "model.gguf", projectionModelPath: "mmproj.gguf");
  ```

### New Features

- **Model downloading** — Load models directly from Hugging Face at runtime using `hf://` URLs (e.g. `hf://owner/repo/model.gguf`). Also supports plain HTTP/HTTPS URLs. Models are cached locally and re-used on subsequent loads. Works on Android with proper cache directory selection.
- **Audio input support** — Added `AudioPart` for multimodal prompts. You can now send audio alongside text and images to models that support it.
- **Load sampler settings from GGUF** — Sampler configuration (temperature, top_k, top_p, min_p, XTC, repetition penalties, mirostat) is now automatically read from GGUF metadata when present, so models ship with their recommended sampling settings out of the box.

### Improvements

- Internal test fixes and cleanup

## 0.7.0-rc2

- Re-work model downloading to pick proper directory on android

## 0.7.0-rc1

- Test build of runtime model downloading for flutter

## 0.6.0

- Gemma 4 support
- Automatic memory usage estimation and splitting of large models across GPU and CPU

## 0.5.3-rc1

- Bump llama.cpp to get Gemma4 support

## 0.5.2

- Fix duplicate image processing
- Improve model selection docs
- Lower dart sdk version

## 0.5.1

- Fix incorrect linking of stdcxx on android
- Fix bad build caching on android build

## 0.5.1-rc1

- Fix incorrect linking of stdcxx on android

## 0.5.0

- Support image ingestion for multimodal vision models
- Fix windows dart executable path resolution (thanks to @leonludwig)

## 0.4.0

### New Features

- Add support for `Set` and `Map` types in Flutter tool calling arguments
- Add support for `num` type in tool argument parsing
- Add FunctionGemma tool calling support
- Add Ministral 3 tool calling support
- Add composable GBNF grammar system for more robust constrained generation (via core)
- System prompt is now optional — omitting it preserves the model's built-in default instead of overwriting with an empty string
- Add Qwen3-style sampling configuration as the new default, replacing mirostat.

### Bug Fixes

- Fix crash when chat history is cleared/reset to empty messages
- Fix stale logits bug after resetting context
- Fix Qwen grammar bug that prevented models from making multiple tool calls in a sequence
- Preserve symlinks when copying xcframework, fixing broken iOS/macOS builds
- Move x86 architecture exclusion into podspec so consumers don't need to add it manually
- Fix context pruning for hybrid transformer/RNN models
- Static link libstdc++ for Android builds, removing NDK runtime dependency

### Improvements

- Switch from static `.a` files to dynamic `.dylib` files in xcframework for iOS/macOS
- Remove minimum macOS version constraint from podspec
- Add worker guard to properly drop child threads on exit, preventing resource leaks
- Prepend grammar step to the sampling chain for correct constraint ordering
- Unified Tool and ToolCall serialization following the HuggingFace standard
- Bump llama.cpp and migrate to new token decoding API
- Improved pub.dev README and documentation
- Removed bundled example app (available separately)

## 0.3.2-rc3

* Statically link stdcxx for android builds to avoid depending on stdcxx from ANDROID_NDK at build-time

## 0.3.2-rc2

* Add config to exclude x86_64 and i386 ios simulators to the ios podspec

## 0.3.2-rc1

* Change MacOS and iOS podspec files to copy .xcframework with -R, to preserve symlinks

## 0.3.1

* Change MacOS and iOS releases to use dynamic linking

## 0.3.0

* Add support for tool parameters with composite types (e.g. List<List<int>>)
* Fix CI/CD for targets that depend on the XCFramework files (MacOS + iOS)

## 0.2.0

* Add option to provide descriptions for individual parameters in Tool constructor.
* Remove slow trigger_word grammar triggers, significantly speeding up generation of long messages when tools are present
* Default to add_bos=true if GGUF file does not specify

## 0.1.1

* Set up automated publishing from CI

## 0.1.0

* Initial release!
