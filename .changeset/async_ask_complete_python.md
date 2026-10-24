---
section: changed
bindings:
  python: major
---

`ask` and `complete` now prefill the context before returning the token stream, instead of prefilling when the stream is first read, and raise a `ValueError` straight away if prefill fails.

On `ChatAsync` they must now be awaited to get the token stream:

```diff
 from nobodywho import ChatAsync

 chat = ChatAsync("./model.gguf")
-response = chat.ask("Is water wet?")
+response = await chat.ask("Is water wet?")
 async for token in response:
     print(token, end="")
```
