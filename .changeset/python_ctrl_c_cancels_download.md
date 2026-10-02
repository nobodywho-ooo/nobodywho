---
section: fixed
bindings:
  python: patch
---

Pressing Ctrl+C during a synchronous GGUF model download now cancels the download, raises `KeyboardInterrupt`, and removes the incomplete temporary file.
