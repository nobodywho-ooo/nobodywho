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

On Apple Silicon Macs, iPhones and iPads, `"auto"` model selection no longer counts the device's memory twice. It used to add the GPU's share of memory on top of the same RAM, so it could pick a model too big to leave room for anything else, such as Qwen3.6-27B instead of Gemma 4 12B on a 24 GB Mac.
