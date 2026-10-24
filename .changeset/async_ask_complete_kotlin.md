---
section: changed
bindings:
  kotlin: major
---

`ask` and `complete` now prefill the context before returning the token stream, instead of prefilling when the stream is first read.

They are now `suspend` functions, which throw if prefill fails, so they must be called from a coroutine:

```diff
-fun startAnswer(chat: Chat): TokenStream =
+suspend fun startAnswer(chat: Chat): TokenStream =
     chat.ask("Is water wet?")
```
