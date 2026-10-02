---
section: removed
bindings:
  python: major
  godot: major
  flutter: major
  kotlin: major
  react-native: major
  swift: major
---

The deprecated `SamplerPresets.grammar()` preset and `SamplerBuilder.grammar()` step are gone, along with the `{"type": "grammar"}` entry in a serialized config. Both have been deprecated since June 2026 in favour of `constrain_with_grammar()`, which accepts the same GBNF as well as Lark and takes the faster llguidance path — switch to it and drop the `root` argument, which was always `"root"` in practice. The one thing it cannot express is a lazy grammar: `trigger_on`, which let the model write freely until a marker before the grammar took effect, has no llguidance equivalent and is removed with no replacement. Godot's method was `set_sampler_preset_grammar()`.
