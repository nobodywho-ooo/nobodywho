---
section: removed
bindings:
  python: major
  flutter: major
  kotlin: major
  react-native: major
  swift: major
---

The `lark_with_slices` sampler step is gone. Nothing constructed it, so the only way to have one is a hand-written sampler config, and `SamplerConfig.from_json()` now rejects a payload containing `{"type": "lark_with_slices"}`. Change it to `{"type": "lark"}` to keep the same grammar.
