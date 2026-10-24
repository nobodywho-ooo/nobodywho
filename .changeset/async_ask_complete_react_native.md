---
section: changed
bindings:
  react-native: major
---

`ask` and `complete` now prefill the context before returning the token stream, instead of prefilling when the stream is first read.

They now return a promise of the token stream, which rejects if prefill fails, so they must be awaited:

```diff
 const chat = await Chat.fromPath({ modelPath: "/path/to/model.gguf" });
-const response = chat.ask("Is water wet?");
+const response = await chat.ask("Is water wet?");
 for await (const token of response) {
   console.log(token);
 }
```
