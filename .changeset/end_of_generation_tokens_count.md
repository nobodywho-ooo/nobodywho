---
section: changed
bindings:
  python: minor
---

The output token counts that `chat.completions.create()` and `responses.create()` report now include the EOG token that ends each generation. This is in line with what providers like Open Router and OpenAI do. It means a response reports one more token than before + roughly one for every tool call.
