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
