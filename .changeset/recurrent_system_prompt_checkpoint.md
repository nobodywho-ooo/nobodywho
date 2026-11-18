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

Hybrid and recurrent models such as Qwen3.5 and LFM2 no longer read the whole system prompt again after `reset_history()` or when a new conversation starts with the same system prompt.
