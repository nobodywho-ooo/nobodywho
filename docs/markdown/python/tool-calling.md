---
title: Tool Calling
description: NobodyWho is a lightweight, open-source AI engine for local LLM inference. Simple, privacy oriented with no infrastructure needed.
sidebar_title: Tool Calling
order: 2
---

To give your LLM the ability to interact with the outside world, you will need tool calling.

!!! info ""
    Note that **not every model** supports tool calling. If the model does not have
    such an option, it might not call your tools.
    For reliable tool calling, we recommend trying the [Qwen](https://huggingface.co/Qwen/models) family of models.

## Declaring a tool

A tool can be created from any (synchronous) python function, which returns a string.
To perform the conversion, all that is needed, is using simple `@tool` decorator. To get
a good sense of how such a tool can look like, consider this geometry example:

```python
import math
from nobodywho import tool, Chat

@tool(description="Calculates the area of a circle given its radius")
def circle_area(radius: float) -> str:
    area = math.pi * radius ** 2
    return f"Circle with radius {radius} has area {area:.2f}"
```

As you can see, every `@tool` definition has to be complemented by a description
of what such tool does. To let your LLM use it, simply add it when creating `Chat`:

```{.python continuation}
chat = Chat('./model.gguf', tools=[circle_area])
```

NobodyWho then figures out the right tool calling format, inspects the names and types of the parameters,
and configures the sampler so that when the model decides to use tools, it will adhere to the format.

Naturally, more tools can be defined and the model can chain the calls for them:

```python
import os
from pathlib import Path
from nobodywho import Chat, tool

@tool(description="Gets path of the current directory")
def get_current_dir() -> str:
    return os.getcwd()

@tool(description="Lists files in the given directory", params={"path": "a relative or absolute path to a directory"})  
def list_files(path: str) -> str:
    files = [f.name for f in Path(path).iterdir() if f.is_file()]
    return f"Files: {', '.join(files)}"

@tool(description="Gets the size of a file in bytes")
def get_file_size(filepath: str) -> str:
    size = Path(filepath).stat().st_size
    return f"File size: {size} bytes"

chat = Chat('./model.gguf', tools=[get_current_dir, list_files, get_file_size])
response = chat.ask('What is the biggest file in my current directory?').completed()
print(response) # The largest file in your current directory is `model.gguf`.
```

## Providing params descriptions

When a tool call is declared, information about the description, the types and the parameters is provided to the model, so it knows it can use it. Crucially, also parameter names are provided.

If those are not enough, you can decide to provide additional information by the `params` parameter:
```python
from nobodywho import tool
@tool(
    description="Given a longitude and latitude, gets the current temperature.",
    params={
        "lon": "Longitude - that is the vertical one!",
        "lat": "Latitude - that is the horizontal one!"
    }
)
def get_current_temperature(lon: str, lat: str) -> str:
    ...
```
These will be then appended to the information provided to model, so it can better navigate itself
when using the tool.
