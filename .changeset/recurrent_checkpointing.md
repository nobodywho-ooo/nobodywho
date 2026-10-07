---
section: added
bindings:
  python: minor
  flutter: minor
  godot: minor
  kotlin: minor
  react-native: minor
  swift: minor
---

Chats with recurrent and hybrid models, such as Qwen3.5, no longer re-read the whole conversation on most turns. The chat now saves the model state after each user message and rewinds to it, where it used to start over whenever an earlier answer was re-rendered differently, for example when thinking is dropped from the history.

MTP speculative decoding now also works with recurrent and hybrid models, where it used to fail when a draft was rejected. For Qwen3.5, which keeps its MTP layers in the model file, pass the model file itself as the draft model.
