---
section: fixed
bindings:
  python: patch
  godot: patch
  flutter: patch
  kotlin: patch
  react-native: patch
  swift: patch
---

A Hugging Face model path without a file name, such as `hf://owner/repo`, now fails with an error that shows the expected `hf://owner/repo/filename.gguf` format instead of a raw parser error.
