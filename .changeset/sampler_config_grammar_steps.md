---
section: changed
bindings:
  python: major
  godot: major
  flutter: major
  kotlin: major
  react-native: major
  swift: major
---

`SamplerConfig` holds its constraints in a new `grammar_steps` list instead of mixing them into `steps`, and `json_schema`, `regex` and `lark` are now grammar steps rather than shift steps. The chain runs the grammar steps, then the shift steps, then the sample step, so a grammar cannot end up behind a truncation step that leaves it nothing valid to pick. Shift steps run in the order you add them, which matters for `dry`, `penalties` and `logit_bias`: they only reshuffle a shortlist if you chain them after a truncation step. Note that llama.cpp's own default chain leads with the penalties.
