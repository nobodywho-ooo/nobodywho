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

A FunctionGemma tool call whose argument value spans multiple lines is no longer dropped. The tool-call grammar lets a value contain newlines (a file body, a code snippet), but the extractor stopped at the first newline and discarded the whole call, so no tool ran. Multi-line values are now parsed.
