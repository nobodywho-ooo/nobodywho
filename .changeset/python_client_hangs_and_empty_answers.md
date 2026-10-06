---
section: fixed
bindings:
  python: patch
---

`chat.completions.create()` and `responses.create()` no longer hang, and no longer occasionally return an empty answer. They hung on any answer of 32 tokens or more, and on shorter ones when Python logging was set to `INFO` or lower or when no other chat had answered in the process yet. With `stream=True`, chunks now arrive as they are generated, rather than all at once after the whole answer.
