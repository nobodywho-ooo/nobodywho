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

Conversation history as rendered to the LLM now matches exactly what the model saw and wrote. Previously some special tokens and whitespace could be added or lost. This can change output, but probably won't do so much.
