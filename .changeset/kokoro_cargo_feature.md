---
section: added
bindings:
  flutter: minor
  godot: minor
  kotlin: minor
  python: minor
  react-native: minor
  swift: minor
---

The Kokoro text-to-speech backend and its `espeak-ng` dependency (GPL-3.0-or-later) are now behind the `kokoro` Cargo feature. It stays on by default, so prebuilt packages are unchanged. Build from source with `--no-default-features` to leave Kokoro and `espeak-ng` out.
