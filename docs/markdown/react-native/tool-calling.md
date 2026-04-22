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

A tool is created by providing a name, description, parameter schemas, and a callback function.
Any regular function can be used as a tool — arguments from the LLM are passed positionally
in the same order as the `parameters` array:

```typescript
import { Tool } from "react-native-nobodywho";

function circleArea(radius: number): string {
  const area = Math.PI * radius * radius;
  return `Circle with radius ${radius} has area ${area.toFixed(2)}`;
}

const circleAreaTool = new Tool({
  name: "circle_area",
  description: "Calculates the area of a circle given its radius",
  parameters: [
    { name: "radius", type: "number", description: "The radius of the circle" },
  ],
  call: circleArea,
});
```

Every `Tool` needs a callback function, a name, a description of what the tool does, and a `parameters` array describing its inputs. Each parameter uses [JSON Schema](https://json-schema.org/) properties (`type`, `enum`, `description`, etc.) plus a `name` field. Arguments are passed to your function positionally in array order.

To let your LLM use it, simply add it when creating `Chat`:

```typescript
import { Chat } from "react-native-nobodywho";

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  tools: [circleAreaTool],
});
```

NobodyWho then figures out the right tool calling format and configures the sampler.

Naturally, more tools can be defined and the model can chain the calls for them:

```typescript
import { Chat, Tool } from "react-native-nobodywho";

function getCurrentDir(): string {
  return "/home/user/documents";
}

function listFiles(path: string): string {
  // In a real app, you'd read the filesystem here
  return "Files: report.pdf, notes.txt, model.gguf";
}

function getFileSize(filepath: string): string {
  // In a real app, you'd check the actual file size
  return `File size: 1024 bytes`;
}

const getCurrentDirTool = new Tool({
  name: "get_current_dir",
  description: "Gets path of the current directory",
  parameters: [],
  call: getCurrentDir,
});

const listFilesTool = new Tool({
  name: "list_files",
  description: "Lists files in the given directory",
  parameters: [
    { name: "path", type: "string", description: "The path to the directory to list" },
  ],
  call: listFiles,
});

const getFileSizeTool = new Tool({
  name: "get_file_size",
  description: "Gets the size of a file in bytes",
  parameters: [
    { name: "filepath", type: "string", description: "The path to the file" },
  ],
  call: getFileSize,
});

const chat = await Chat.fromPath({
  modelPath: "/path/to/model.gguf",
  tools: [getCurrentDirTool, listFilesTool, getFileSizeTool],
});

const response = await chat
  .ask("What is the biggest file in my current directory?")
  .completed();
console.log(response);
```

## Tool calling and the context

As with everything made to improve response quality, using tool calls fills up the context faster than simply chatting with an LLM. So be aware that you might need to use a larger context size than expected when using tools.
