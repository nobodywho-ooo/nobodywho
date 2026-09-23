---
section: fixed
bindings:
  python: patch
  godot: patch
  flutter: patch
  kotlin: patch
  react-native: patch
  swift: patch
---

Kokoro speech synthesis no longer garbles contractions written with typographic apostrophes (`’`, `‘`, `´`, `` ` ``) or curly double quotes (`“ ”`), which word processors and phone autocorrect produce — `it’s` was spoken "it-ess", `don’t` "don-tee". They now fold to their ASCII counterparts before phonemization, as the supertonic backend already did.
