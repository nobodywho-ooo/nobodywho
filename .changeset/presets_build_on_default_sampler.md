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

Every `SamplerPresets` entry now builds on the default sampler (top-k 20, top-p 0.95, temperature 0.6) and changes one thing, instead of producing a chain holding only its own step: enabling a constraint no longer drops the truncation and temperature you would otherwise be sampling with, and `top_k`, `top_p` and `temperature` each override their counterpart and leave the rest alone. Constrained output is valid as before but less random within the constrained set. `greedy` is unaffected — it needs no shift steps.
