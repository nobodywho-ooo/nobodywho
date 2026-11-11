---
section: fixed
bindings:
  python: minor
  godot: minor
---

`Encoder` and `CrossEncoder` now wait for the context to be created and raise an error if that fails, instead of failing on first use.
