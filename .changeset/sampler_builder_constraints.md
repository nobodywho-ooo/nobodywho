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

`SamplerBuilder` gained `constrain_with_json_schema`, `constrain_with_regex`, `constrain_with_grammar` and `json`, so a constraint can be combined with a temperature or a repetition penalty — the equivalent `SamplerPresets` each produce a finished sampler and cannot be layered.
