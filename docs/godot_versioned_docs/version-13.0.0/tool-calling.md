---
title: Tool Calling
description: Let the LLM call your GDScript functions.
sidebar_position: 2
---

The LLM can call functions in your game — flip switches, query inventory, run game logic — and
then reason about the results. You declare tools with `NobodyWhoTool` and hand them to the chat;
the model decides when to call them.

## Declaring a tool

The simplest way is `NobodyWhoTool.create` with one of your methods. The tool's name is the method
name, and its schema is derived from the parameter **type hints** — so annotate every parameter
with `int`, `float`, `bool`, `String`, or `Array`:

```gdscript
func get_magic_word(box_name: String) -> String:
    # your game logic here
    return "sesame"

func _ready():
    var tool = NobodyWhoTool.create(get_magic_word, "Gets the magic word for a box.")
    var chat = await NobodyWhoChat.create("./model.gguf", {"tools": [tool]})
    var stream = chat.ask("Open the red box using the magic word.")
    print(await stream.completed())
```

The model asks for the tool with the arguments it wants, NobodyWho calls your method, and the
return value is fed back so the model can finish its answer.

Return values become strings the model sees: `String` values pass through directly, and other
values are JSON-encoded — a returned Dictionary becomes a JSON object the model can read.

### Async tools

A tool can also be a coroutine — useful when the action itself has to await something (an
animation, a signal, another model). Just make the method async:

```gdscript
func press_button(color: String) -> String:
    await get_tree().create_timer(0.5).timeout # let the animation play
    return "the %s button lit up" % color
```

NobodyWho waits for the coroutine to finish and feeds its return value to the model.

:::warning
Tool calls have no timeout. A tool must eventually return; otherwise its chat remains blocked and
its response stream never completes. `stop_generation()` cannot stop a tool that is already
running.
:::

### Providing a schema manually

Lambdas have no type hints to infer from, and some schemas need more than hints can express —
enums, nested objects, parameter descriptions, optional fields. For those, use
`create_with_schema` with a JSON schema (a Dictionary, or a JSON string):

```gdscript
var tool = NobodyWhoTool.create_with_schema(
    "press_button", # tool name, as the model will see it
    "Press the button of the given color. Returns what happened.",
    {
        "type": "object",
        "properties": {
            "color": {
                "type": "string",
                "enum": ["red", "green", "blue"],
                "description": "Which button to press.",
            },
        },
        "required": ["color"],
    },
    func(color: String) -> String: return "the %s button lit up" % color,
)
```

The schema's `properties` keys must match the callable's parameter names; missing arguments
arrive as `null`.

## Pre-packaged tools

NobodyWho ships two sandboxed interpreter tools that need no game code. The sandbox has **no
access** to the filesystem, the network, or environment variables:

- `NobodyWhoTool.python()` — a Python interpreter. Optional limits (0 = no limit):
  `max_duration_secs`, `max_memory_bytes`, `max_recursion_depth`.
- `NobodyWhoTool.bash()` — an in-memory bash shell, `max_commands` optional.

```gdscript
var tool = NobodyWhoTool.python()
var chat = await NobodyWhoChat.create("./model.gguf", {"tools": [tool]})
var stream = chat.ask("What is 6 times 7? Use the run_python tool to compute it.")
print(await stream.completed())
```

These are great for "let the model do math" style tasks without writing glue code yourself.

## Tool calling and the context

Tool calls and their results are stored in the chat history, just like normal messages — the
model remembers what it did. That also means they consume context, so a chatty tool-loop eats
tokens; see [Chat](./chat#context) for context management.

To change the available tools on a live chat, call `set_tools()`:

```gdscript
await chat.set_tools([tool1, tool2])
```

:::info
A tool that calls **back into its own chat** (asking a question while the chat is waiting for the
tool to return) can never complete — NobodyWho detects this and fails fast with an error instead
of hanging. Use a second chat instance if a tool needs model inference.
:::
