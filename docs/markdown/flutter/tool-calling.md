---
title: Tool Calling
description: An introduction to tools in NobodyWho
sidebar_title: Tool Calling
order: 2
---

To give your LLM the ability to interact with the outside world, you will need tool calling.

!!! info ""
    Note that **not every model** supports tool calling. If the model does not have
    such an option, it might not call your tools.
    For reliable tool calling, we recommend trying the [Qwen](https://huggingface.co/Qwen/models) family of models.

## Declaring a tool

A tool can be created from any Dart function that returns a `String` or `Future<String>`.
To perform the conversion, you simply need to use the `describeTool()` function. To get
a good sense of how such a tool can look like, consider this geometry example:

```dart
import 'dart:math' as math;
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

final circleAreaTool = nobodywho.Tool(
  name: "circle_area",
  description: "Calculates the area of a circle given its radius",
  function: ({ required double radius }) {
    final area = math.pi * radius * radius;
    return "Circle with radius $radius has area ${area.toStringAsFixed(2)}";
  }
);
```

As you can see, every `Tool()` call needs a function, a name, and a description
of what such tool does. To let your LLM use it, simply add it when creating `Chat`:

``` {.dart continuation}
final chat = nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  tools: [circleAreaTool]
);
```

NobodyWho then figures out the right tool calling format, inspects the names and types of the parameters,
and configures the sampler.

Naturally, more tools can be defined and the model can chain the calls for them:

```dart
import 'dart:io';
import 'package:nobodywho_dart/nobodywho_dart.dart' as nobodywho;

final getCurrentDirTool = nobodywho.Tool(
  name: "get_current_dir",
  description: "Gets path of the current directory",
  function: () => Directory.current.path
);

final listFilesTool = nobodywho.Tool(
  name: "list_files",
  description: "Lists files in the given directory",
  function: ({required String path}) {
    final dir = Directory(path);
    final files = dir.listSync()
        .where((entity) => entity is File)
        .map((file) => file.path.split('/').last)
        .toList();
    return "Files: ${files.join(', ')}";
  }
);

final getFileSizeTool = nobodywho.Tool(
  name: "get_file_size",
  description: "Gets the size of a file in bytes",
  function: ({required String filepath}) async {
    final file = File(filepath);
    final size = await file.length();
    return "File size: $size bytes";
  }
);

final chat = await nobodywho.Chat.fromPath(
  modelPath: './model.gguf',
  tools: [getCurrentDirTool, listFilesTool, getFileSizeTool],
  allowThinking : false
);

final response = await chat.ask('What is the biggest file in my current directory?').completed();
print(response); // The largest file in your current directory is `model.gguf`.
```

## Tool calling and the context

As with everything made to improve response quality, using tool calls fills up the context faster than simply chatting with an LLM. So be aware that you might need to use a larger context size than expected when using tools.

