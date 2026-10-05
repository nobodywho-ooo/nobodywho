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

Speech-to-text no longer mixes up languages when one `SpeechToText` transcribes several files, or audio longer than 30 seconds. Language detection ran on whatever the previous transcription had left behind, so English could come back in French or another language. A transcript that never finishes (a model stuck repeating itself) is now cut off at Whisper's length limit instead of failing with an ONNX Runtime reshape error.
