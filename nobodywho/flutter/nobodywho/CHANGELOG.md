## 2.2.0

### Improved error messages (#532)

Clearer, more actionable errors for the three places users most often hit trouble: model loading, model downloading, and context shifting. Messages now point at the likely cause (bad path, network failure, OOM, context window exhausted) instead of surfacing raw lower-level errors.

### watchOS build fix (#527)

Updated the `llama-cpp-rs` fork so Metal isn't built for watchOS targets, unblocking watchOS builds.

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
