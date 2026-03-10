## 0.4.1-rc1

- Try to reduce target iOS version to 16.0

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
