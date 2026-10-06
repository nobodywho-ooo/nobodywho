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

Chats no longer leave stray tokens in the model's context after a reply. The chat assumed the model had read the whole rendered reply, including template text after the end-of-turn token that was never decoded, so later turns could land one token off and keep stale text in context (seen with Gemma 3).
