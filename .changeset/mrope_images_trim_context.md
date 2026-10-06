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

Chats with an image on Qwen2-VL, Qwen2.5-VL, Qwen3-VL, Qwen3.5 or PaddleOCR-VL no longer keep stale text in the model's context. These models give an image fewer context positions than tokens, which the chat didn't account for, so after an image the old end of the conversation was never removed and new text was added after it. The context-full check and the reported context usage now also count all of an image's tokens.
