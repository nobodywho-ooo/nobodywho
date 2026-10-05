---
title: Tool Calling
description: An introduction to tools in NobodyWho
sidebar_position: 2
---

To give your LLM the ability to interact with the outside world, you will need tool calling.

:::info
Note that **not every model** supports tool calling. If the model does not have
such an option, it might not call your tools.
For reliable tool calling, we recommend trying the [Qwen](https://huggingface.co/collections/NobodyWho/qwen-3) family of models.

:::

## Declaring a tool

Create a tool by passing a delegate — a lambda, a local function or a method group. NobodyWho uses reflection to inspect the parameter names and types, and turns them into the tool's JSON schema:

```csharp
using NobodyWho;

var weatherTool = new Tool(
    "get_weather",
    "Get the current weather for a city",
    (string city, string unit) => $$"""{"temp": 22, "unit": "{{unit}}"}""");
```

To let your LLM use the tool, pass it when creating `Chat`:

```csharp
using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    tools: [weatherTool]);
```

NobodyWho figures out the right tool calling format, inspects the names and types of the parameters, and configures the sampler. When the model calls the tool, its arguments are converted back to your parameter types before your function runs.

Supported parameter types: `string`, `bool`, the integer and floating-point types (`int`, `long`, `double`, `float`, `decimal`, ...), enums, nullable versions of these, arrays and lists of a supported type (`string[]`, `List<int>`), and dictionaries with `string` keys (`Dictionary<string, int>`). The function must return `string`, `Task<string>` or `ValueTask<string>`. An unsupported signature throws an `ArgumentException` when you create the `Tool`. Every parameter is required in the schema the model sees, even one with a default value, and a tool that returns `null` gives the model an empty string.

## Providing parameter descriptions

The model sees the tool's description and the names and types of its parameters. If those are not enough, describe a parameter with `[Description]` from `System.ComponentModel`:

```csharp
using System.ComponentModel;
using NobodyWho;

var temperatureTool = new Tool(
    "get_current_temperature",
    "Given a longitude and latitude, gets the current temperature.",
    ([Description("Longitude - that is the vertical one!")] double lon,
     [Description("Latitude - that is the horizontal one!")] double lat) => "21.5");
```

The description is added to the parameter's schema, so the model can better navigate itself when using the tool. Enums are a good way to limit a parameter to a fixed set of values — the model can only pick one of the enum's names.

## Multiple tools

Naturally, more tools can be defined and the model can chain calls:

```csharp
static string GetCurrentDir() => Directory.GetCurrentDirectory();

static string ListFiles(string path) => string.Join(", ", Directory.GetFiles(path));

static string GetFileSize(string filepath) => $"File size: {new FileInfo(filepath).Length} bytes";

using var chat = await Chat.FromPathAsync(
    "./model.gguf",
    tools: [
        new Tool("get_current_dir", "Gets the current directory", GetCurrentDir),
        new Tool("list_files", "Lists files in a directory", ListFiles),
        new Tool("get_file_size", "Gets the size of a file", GetFileSize),
    ]);

string response = await chat.Ask("What is the biggest file in my current directory?").CompletedAsync();
Console.WriteLine(response);
```

## Async functions

Async functions work as tools too — return `Task<string>` or `ValueTask<string>`:

```csharp
using var http = new HttpClient();

var fetchTool = new Tool(
    "fetch_data",
    "Fetch data from an API",
    async (string query) => await http.GetStringAsync($"https://api.example.com?q={Uri.EscapeDataString(query)}"));
```

The tool runs on NobodyWho's inference thread, which waits for it to finish, so your own threads and UI remain responsive while the tool executes.

## Errors in tools

If a tool throws, the exception does not escape into your code. Instead the model is shown `Error: ` followed by the exception's message, so it can recover — for example by calling the tool again with different arguments:

```csharp
var divideTool = new Tool(
    "divide",
    "Divide two numbers",
    (double a, double b) => b == 0
        ? throw new ArgumentException("Cannot divide by zero.")
        : (a / b).ToString());
```

Keep exception messages short and useful to the model; it sees nothing else.

## Writing the schema yourself

If a parameter cannot be described by a .NET type — say you need a JSON schema `minimum`, or a pattern — use the overload that takes a list of `ToolParameter`s and a callback receiving the raw arguments as a `JsonElement`:

```csharp
using System.Text.Json;
using NobodyWho;

var searchTool = new Tool(
    "search",
    "Search the product catalogue",
    [
        new ToolParameter("query", """{"type": "string", "description": "What to look for"}"""),
        new ToolParameter("limit", """{"type": "integer", "minimum": 1, "maximum": 20}"""),
    ],
    (JsonElement args) =>
    {
        string query = args.GetProperty("query").GetString()!;
        int limit = args.GetProperty("limit").GetInt32();
        return $"Found {limit} results for '{query}'.";
    });
```

All parameters given this way are required. To see the schema the model gets for any tool, call `tool.GetSchemaJson()`.

## Changing tools at runtime

You can change the available tools on an existing chat:

```csharp
await chat.SetToolsAsync([newTool1, newTool2]);
```

Or reset the context with new tools:

```csharp
await chat.ResetContextAsync(systemPrompt: "Updated prompt", tools: [newTool]);
```

## Tool calling and the context

As with most things made to improve response quality, using tool calls fills up the context faster than simply chatting with an LLM. So be aware that you might need to use a larger context size than expected when using tools.
