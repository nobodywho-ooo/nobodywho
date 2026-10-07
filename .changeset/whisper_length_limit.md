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

Speech-to-text no longer fails with an ONNX Runtime reshape error when a transcript never finishes (a model stuck repeating itself); the transcript is cut off at Whisper's length limit instead.
