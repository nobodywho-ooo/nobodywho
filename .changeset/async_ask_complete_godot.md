---
section: changed
bindings:
  godot: major
---

`ask` and `complete` now prefill the context before returning the token stream, instead of prefilling when the stream is first read.

They must now be awaited to get the token stream, which is null if prefill fails:

```diff
 var chat = await NobodyWhoChat.create("./model.gguf", {})
-var stream = chat.ask("Is water wet?")
+var stream = await chat.ask("Is water wet?")
 print(await stream.completed())
```
