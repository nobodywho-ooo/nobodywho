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

Create a tool by passing a function reference. NobodyWho uses Kotlin reflection to automatically inspect parameter names and types:

```kotlin
import ai.nobodywho.Tool

fun getWeather(city: String, unit: String): String {
    return """{"temp": 22, "unit": "$unit"}"""
}

val weatherTool = Tool(
    name = "get_weather",
    description = "Get the current weather for a city",
    function = ::getWeather
)
```

To let your LLM use the tool, pass it when creating `Chat`:

```kotlin
val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    tools = listOf(weatherTool)
)
```

NobodyWho figures out the right tool calling format, inspects the names and types of the parameters, and configures the sampler.

Supported parameter types: `String`, `Int`, `Long`, `Double`, `Float`, `Boolean`, and collections like `List<String>` or `Map<String, Int>`.

## Multiple tools

Naturally, more tools can be defined and the model can chain calls:

```kotlin
fun getCurrentDir(): String {
    return System.getProperty("user.dir") ?: "."
}

fun listFiles(path: String): String {
    return java.io.File(path).list()?.joinToString(", ") ?: "empty"
}

fun getFileSize(filepath: String): String {
    return "File size: ${java.io.File(filepath).length()} bytes"
}

val chat = Chat.fromPath(
    modelPath = "./model.gguf",
    tools = listOf(
        Tool(name = "get_current_dir", description = "Gets the current directory", function = ::getCurrentDir),
        Tool(name = "list_files", description = "Lists files in a directory", function = ::listFiles),
        Tool(name = "get_file_size", description = "Gets the size of a file", function = ::getFileSize),
    )
)

val response = chat.ask("What is the biggest file in my current directory?").completed()
println(response)
```

## Suspend functions

Suspend functions work as tools too — just pass the reference the same way:

```kotlin
suspend fun fetchData(query: String): String {
    return httpClient.get("https://api.example.com?q=$query").bodyAsText()
}

val fetchTool = Tool(
    name = "fetch_data",
    description = "Fetch data from an API",
    function = ::fetchData
)
```

Your UI and other coroutines remain responsive while the tool executes.

## Function scope limitation

The `Tool` class uses Kotlin reflection (`KFunction`) to introspect parameter names and types. This requires the function to be a **top-level function**, a **class method**, or a **companion object method**.

Local functions defined inside other functions or coroutines are **not supported** — the Kotlin compiler mangles their JVM signatures, which breaks reflection.

```kotlin
// ✅ Works — top-level function
fun ping(): String = "pong"
val pingTool = Tool(name = "ping", description = "Ping", function = ::ping)

// ✅ Works — companion object method
class MyTools {
    companion object {
        fun ping(): String = "pong"
    }
}
val pingTool = Tool(name = "ping", description = "Ping", function = MyTools::ping)

// ❌ Does NOT work — local function inside a coroutine
suspend fun example() {
    fun ping(): String = "pong"  // This will fail at runtime
    val tool = Tool(name = "ping", description = "Ping", function = ::ping)
}
```

## Changing tools at runtime

You can change the available tools on an existing chat:

```kotlin
chat.setTools(listOf(newTool1, newTool2))
```

Or reset the context with new tools:

```kotlin
chat.resetContext(systemPrompt = "Updated prompt", tools = listOf(newTool))
```

## Tool calling and the context

As with most things made to improve response quality, using tool calls fills up the context faster than simply chatting with an LLM. So be aware that you might need to use a larger context size than expected when using tools.
