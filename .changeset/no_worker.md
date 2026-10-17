---
section: changed
bindings:
  python: minor
  godot: minor
  flutter: minor
  kotlin: minor
  react-native: minor
  swift: minor
---

Removed internal worker thread, instead a new worker thread is only spawned when needed (such as when calling `.ask` or `.complete`).
