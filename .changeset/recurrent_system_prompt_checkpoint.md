---
section: changed
bindings:
  python: minor
  godot: minor
  flutter: minor
  kotlin: minor
  react-native: minor
  swift: minor
---

Hybrid and recurrent models such as Qwen3.5 and LFM2 now cache the system prompt specifically, so answers are faster after `reset_history()` or when the chat history is replaced with a new conversation, as long as the system prompt, tools and template variables are unchanged.
