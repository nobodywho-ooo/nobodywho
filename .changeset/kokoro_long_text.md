---
section: fixed
bindings:
  python: patch
  flutter: patch
  godot: patch
  kotlin: patch
  react-native: patch
  swift: patch
---

Kokoro text-to-speech reads text of any length. It used to fail with "Input is N phonemes; max 509" for anything over a few sentences; now it reads long text a piece at a time, splitting at sentence ends where it can, and returns one recording.
