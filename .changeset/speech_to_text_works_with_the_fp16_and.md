---
section: fixed
bindings:
  python: patch
  flutter: patch
  godot: patch
  kotlin: patch
  react-native: patch
  swift: patch
---

Speech-to-text works with the `fp16` and `q4f16` Whisper quantizations. Before, both failed while the model was loading.
