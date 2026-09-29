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

Qwen3.5, Qwen3.6 and FunctionGemma tool calls now pass an argument to the tool with the type its schema gives. Previously anything that parsed as JSON was converted, so a string parameter given `10` received the number `10` instead of `"10"`.
