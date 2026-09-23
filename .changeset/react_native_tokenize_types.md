---
section: fixed
bindings:
  react-native: patch
---

Type errors in `Chat.tokenize`. Changed `async tokenize(message: string | Prompt): Promise<(number | null)[]>` to `async tokenize(message: string | Prompt): Promise<(number | undefined)[]>`. The `null` type was incorrect, as the embedding slots are represented by `undefined` in the TypeScript binding.
