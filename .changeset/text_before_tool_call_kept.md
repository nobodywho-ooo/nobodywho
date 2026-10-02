---
section: fixed
bindings:
  python: patch
  godot: patch
  flutter: patch
  kotlin: patch
  react-native: patch
  swift: patch
---

Text a model writes before a tool call is now kept in the chat history. Previously whatever a model generated (and streamed) before the tool call was forgotten and not visible in `get_chat_history()`. It is now stored as content in the assistant message and is rendered next to the tool call. Note that the tool call is still stored in history as the function name and its arguments.
