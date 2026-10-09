---
section: changed
bindings:
  python: major
  godot: major
  flutter: major
  kotlin: major
  react-native: major
  swift: major
---

Several changes to how tool calls are read from a model's output:

- With tools, a chat now fails to start if the model's vocabulary doesn't have its format's tool call markers as single special tokens. All supported models have them.
- A tool call cut off by stopping the generation or by the token limit is now kept as text and never runs. An LFM2 call could run before.
- Qwen3.5, Qwen3.6 and FunctionGemma tool calls now pass an argument to the tool with the type its schema gives. Previously anything that parsed as JSON was converted, so a string parameter given `10` received the number `10` instead of `"10"`.
- A tool call marker a model writes inside its reasoning is now part of the reasoning, and no longer starts a tool call.
- A block of tool calls that can't be read is now streamed as text, not only returned in the response.
