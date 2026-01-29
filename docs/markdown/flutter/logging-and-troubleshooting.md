---
title: Logging and Troubleshooting
sidebar_title: Logging
order: 6
---

# Logging and troubleshooting

The Flutter bindings for NobodyWho provide a simple debug logging function.

In short, to enable debug logs:

```dart
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

```

This can be useful for getting some insight into what the model is choosing to do and when.
For example when tool calls are made, when context shifting happens, etc.

Typically, you would call `initDebugLog()` once during your app initialization, before creating any `Chat`, `Encoder`, or `CrossEncoder` instances:

```dart
Future<void> main() async {
  await nobodywho.NobodyWho.init();
  
  // Now create your chat instances, etc.
  final chat = await nobodywho.Chat.fromPath(modelPath: './model.gguf');
  // ...
}
```

