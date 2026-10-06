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

Qwen3.5 chats with an image no longer keep stale text in the model's context. The same applies to the older Qwen2-VL, Qwen2.5-VL and Qwen3-VL, and to PaddleOCR-VL. These models give an image fewer context positions than tokens, which the chat didn't account for, so after an image the old end of the conversation was never removed and new text was added after it. The context-full check and the reported context usage now also count all of an image's tokens.
