---
section: fixed
bindings:
  python: major
  flutter: major
  godot: major
  kotlin: major
  react-native: major
  swift: major
---

ONNX Runtime, used for speech-to-text, text-to-speech and voice activity detection, is updated from 1.24 to 1.28. CUDA acceleration now needs a driver that supports CUDA 13, as CUDA 12 builds are no longer shipped. On platforms without CUDA support, requesting the `cuda` device now fails with "CUDA is not supported on this platform".
