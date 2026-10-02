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

`SamplerPresets.json()` and the new `SamplerBuilder.json()` now constrain with the JSON schema `{"type":"object"}` through llguidance, so they take the same faster per-token path as the other `constrain_with_*` presets. Output is still a JSON object of any shape, as the old grammar's root was an object too. The new grammar is slightly more permissive at the edges: the old one allowed at most one newline plus 20 spaces of indentation per gap, and could not emit exponents like `1e10`.
