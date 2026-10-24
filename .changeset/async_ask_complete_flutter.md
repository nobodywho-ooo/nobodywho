---
section: changed
bindings:
  flutter: major
---

`ask` and `complete` now prefill the context before returning the token stream, instead of prefilling when the stream is first read.

`await chat.ask(...).completed()` still works, but streaming needs an extra `await`:

```diff
 final chat = await nobodywho.Chat.fromPath(modelPath: "./model.gguf");
-await for (final token in chat.ask("Is water wet?")) {
+await for (final token in await chat.ask("Is water wet?")) {
   print(token);
 }
```
