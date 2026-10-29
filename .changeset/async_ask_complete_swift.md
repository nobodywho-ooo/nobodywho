---
section: changed
bindings:
  swift: major
---

`ask` and `complete` now prefill the context before returning the token stream, instead of prefilling when the stream is first read.

They are now `async throws`, and so must be tried and awaited:

```diff
 let chat = try await Chat.fromPath(modelPath: "/path/to/model.gguf")
-let response = chat.ask("Is water wet?")
+let response = try await chat.ask("Is water wet?")
 for try await token in response {
     print(token, terminator: "")
 }
```
