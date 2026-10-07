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

Speech-to-text no longer mixes up languages when one `SpeechToText` transcribes several files, or audio longer than 30 seconds. Language detection ran on whatever the previous transcription had left behind, so audio could be detected as, and transcribed in, the previous language.
