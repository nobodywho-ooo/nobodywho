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
    For reliable tool calling, we recommend trying the [Qwen](https://huggingface.co/collections/NobodyWho/qwen-3) family of models.

## The @DeclareTool macro

The easiest way to create a tool is with the `@DeclareTool` macro. Just annotate any function with a description, and NobodyWho generates the tool for you:

```swift
import NobodyWho

@DeclareTool("Calculates the area of a circle given its radius")
func circleArea(radius: Double) -> String {
    let area = Double.pi * radius * radius
    return "Circle with radius \(radius) has area \(String(format: "%.2f", area))"
}

// The macro generates `circleAreaTool` automatically
let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    tools: [circleAreaTool]
)
```

The generated variable is named `<functionName>Tool` — so `circleArea` becomes `circleAreaTool`.

Supported parameter types: `String`, `Int`, `Double`, `Float`, `Bool`.

### Async tools

The `@DeclareTool` macro also works with async functions. This is useful for tools that need to make network requests, database queries, or other asynchronous operations:

```swift
@DeclareTool("Search the knowledge base")
func search(query: String) async -> String {
    let results = await knowledgeBase.search(query)
    return results.joined(separator: "\n")
}

@DeclareTool("Get the current weather for a city")
func getWeather(city: String, unit: String) async -> String {
    let data = await weatherAPI.fetch(city: city, unit: unit)
    return "{\"temp\": \(data.temp), \"unit\": \"\(unit)\"}"
}

let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    tools: [searchTool, getWeatherTool]
)
```

## Creating tools manually

You can also create tools without the macro, using the `Tool` initializer directly.
Arguments from the LLM are passed positionally in the same order as the `parameters` array:

```swift
let circleAreaTool = Tool(
    name: "circle_area",
    description: "Calculates the area of a circle given its radius",
    parameters: [("radius", "number")]
) { args in
    let radius = args[0] as! Double
    let area = Double.pi * radius * radius
    return "Circle with radius \(radius) has area \(String(format: "%.2f", area))"
}
```

For async callbacks, there's a separate initializer:

```swift
let searchTool = Tool(
    name: "search",
    description: "Search the knowledge base",
    parameters: [("query", "string")]
) { args async in
    let query = args[0] as! String
    return await knowledgeBase.search(query)
}
```

## Multiple tools

Naturally, more tools can be defined and the model can chain the calls for them:

```swift
@DeclareTool("Gets path of the current directory")
func getCurrentDir() -> String {
    return FileManager.default.currentDirectoryPath
}

@DeclareTool("Lists files in the given directory")
func listFiles(path: String) -> String {
    let files = try? FileManager.default.contentsOfDirectory(atPath: path)
    return (files ?? []).joined(separator: ", ")
}

@DeclareTool("Gets the size of a file in bytes")
func getFileSize(filepath: String) -> String {
    let attrs = try? FileManager.default.attributesOfItem(atPath: filepath)
    let size = attrs?[.size] as? Int ?? 0
    return "File size: \(size) bytes"
}

let chat = try await Chat.fromPath(
    modelPath: "/path/to/model.gguf",
    tools: [getCurrentDirTool, listFilesTool, getFileSizeTool]
)

let response = try await chat
    .ask("What is the biggest file in my current directory?")
    .completed()
print(response)
```

## Tool calling and the context

As with most things made to improve response quality, using tool calls fills up the context faster than simply chatting with an LLM. So be aware that you might need to use a larger context size than expected when using tools.
