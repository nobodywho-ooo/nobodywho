---
section: added
bindings:
  python: minor
  godot: minor
  flutter: minor
  kotlin: minor
  react-native: minor
  swift: minor
---

Context shifting is configurable: how many turns to always keep at the start and end, the size to shrink to (a fraction of the context size or a number of tokens), or turning it off so a full context is an error. Pass `ContextShiftOptions` when creating a chat or later with `set_context_shift`. **Godot:** use the `"context_shift"` config key and `set_context_shift()` with a bool or Dictionary.
