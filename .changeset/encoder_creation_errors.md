---
section: fixed
bindings:
  swift: major
  kotlin: major
  react-native: major
  python: major
  godot: major
  flutter: major
---

`Encoder` and `CrossEncoder` now wait for the context to be created and throw/return an error if that fails, instead of failing on first use.
