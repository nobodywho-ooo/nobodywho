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

`SamplerPresets.dry()` now actually applies the DRY penalty. Its multiplier was 0.0, which llama.cpp reads as "disabled", so the preset was a no-op that sampled exactly like the default one. It is now 0.8, the value the preset's other numbers (base 1.75, allowed length 2) are tuned for. The preset leads with its DRY step, so the penalty sees the whole vocabulary rather than what survived truncation.
