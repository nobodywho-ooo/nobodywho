---
title: Logging and Troubleshooting
sidebar_position: 8
---

# Logging and troubleshooting

The bindings integrate with Dart's standard [`package:logging`](https://pub.dev/packages/logging).

To enable debug logs, follow the example from their documentation, and add an `onRecord` listener
before you run `NobodyWho.init()`. Something like the following would work:

```dart
import 'package:nobodywho/nobodywho.dart' as nobodywho;
import 'package:logging/logging.dart';

void main() async {
  // Initialize logger.
  Logger.root.level = Level.ALL;
  Logger.root.onRecord.listen((record) {
    print('${record.level.name}: ${record.time}: ${record.message}');
  });

  // Initialize NobodyWho
  await nobodywho.NobodyWho.init();

  // Rest of application here.
}
```

This can be useful for getting some insight into what the model is choosing to do and when.
For example when tool calls are made, when context shifting happens, etc.
