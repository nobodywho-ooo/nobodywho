---
section: changed
bindings:
  python: major
  flutter: major
  kotlin: major
  react-native: major
  swift: major
---

`SamplerConfig.from_json()` rejects a saved config with a grammar step inside `steps`, naming the step it could not read; move that entry into a `"grammar_steps"` list to load it.
