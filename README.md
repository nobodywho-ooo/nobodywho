![Nobody Who](./assets/banner.png)

[![Discord](https://img.shields.io/discord/1308812521456799765?logo=discord&style=flat-square)](https://discord.gg/qhaMc2qCYB)
[![Matrix](https://img.shields.io/badge/Matrix-000?logo=matrix&logoColor=fff)](https://matrix.to/#/#nobodywho:matrix.org)
[![Mastodon](https://img.shields.io/badge/Mastodon-6364FF?logo=mastodon&logoColor=fff&style=flat-square)](https://mastodon.gamedev.place/@nobodywho)
[![Godot Engine](https://img.shields.io/badge/Godot-%23FFFFFF.svg?logo=godot-engine&style=flat-square)](https://godotengine.org/asset-library/asset/2886)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg?style=flat-square)](CODE_OF_CONDUCT.md) 
[![Docs](https://img.shields.io/badge/Docs-lightblue?style=flat-square)](https://docs.nobodywho.ooo)


NobodyWho is a library that lets you run LLMs locally and efficiently on any device.

We currently support Python and Godot, with more integrations on the way.


## At a Glance

* 🏃 Run any LLM locally, offline, for free
* ⚒️ Fast, simple tool calling - just pass a normal function
* 👌 Guaranteed perfect tool calling every time, automatically derives a grammar from your function signature
* 🗨️ Conversation-aware preemptive context shifting, for lobotomy-free conversations of infinite length
* 💻 Ship optimized native code for multiple platforms: Windows, Linux, macOS, Android
* ⚡ Super fast inference on GPU powered by Vulkan or Metal
* 🤖 Compatible with thousands of pre-trained LLMs - use any LLM in the GGUF format
* 🦙 Powered by the wonderful [llama.cpp](https://github.com/ggml-org/llama.cpp)


## Python

### Quick Start

Start by installing NobodyWho. This is simply

```sh
pip install nobodywho
```

Next download a model. For a quick start we recommend this [one](https://huggingface.co/bartowski/Qwen_Qwen3-0.6B-GGUF/resolve/main/Qwen_Qwen3-0.6B-Q5_K_L.gguf). It is quite small, but will get the job done.

Then you can try to get a response from the model with the following code snippet:
```python
from nobodywho import Chat
chat = Chat("./path/to/your/model.gguf")
response = chat.ask("Is water wet?").completed()
print(response)
```

You can also setup a basic chat bot very quickly with the code snippet below: 

```python
from nobodywho import Chat, TokenStream
chat = Chat("./path/to/your/model.gguf")
while True:
    prompt = input("Enter your prompt: ")
    response : TokenStream = chat.ask(prompt)
    for token in response:
        print(token, end="", flush=True)
    print()

```

### Tool calling

Once you have a chat up and running you will likely want to give it access to tools. This is very easy in NobodyWho:

```python
import math
from nobodywho import tool, Chat

@tool(description="Calculates the area of a circle given its radius")
def circle_area(radius: float) -> str:
    area = math.pi * radius ** 2
    return f"Circle with radius {radius} has area {area:.2f}"

chat = Chat("./path/to/your/model.gguf", tools=[circle_area])
```

Adding tools to your chat like above will automatically make these available to the model.
There plenty of things you can do with tools and many of these are coverend in our docs.

## Godot

You can install it from inside the Godot editor: In Godot 4.5+, go to AssetLib and search for "NobodyWho".

...or you can grab a specific version from our [github releases page.](https://github.com/nobodywho-ooo/nobodywho/releases) You can install these zip files by going to the "AssetLib" tab in Godot and selecting "Import".

Make sure that the ignore asset root option is set in the import dialogue.

For further instructions on how to setup NobodyWho in Godot please refer to our docs.

## Documentation

[The documentation](https://docs.nobodywho.ooo) has everything you might want to know: https://docs.nobodywho.ooo/

## How to Help 

* ⭐ Star the repo and spread the word about NobodyWho!
* Join our [Discord](https://discord.gg/qhaMc2qCYB) or [Matrix](https://matrix.to/#/#nobodywho:matrix.org) communities
* Found a bug? Open an issue!
* Submit your own PR - contributions welcome
* Help improve docs or write tutorials


### Can I export to HTML5 or iOS?

Currently only Linux, MacOS, Android and Windows are supported platforms.

iOS exports seem very feasible. See issue [#114](https://github.com/nobodywho-ooo/nobodywho/issues/114)

Web exports will be a bit trickier to get right. See issue [#111](https://github.com/nobodywho-ooo/nobodywho/issues/111).


## Licensing

There has been some confusion about the licensing terms of NobodyWho. To clarify:

> Linking two programs or linking an existing software with your own work does not – at least under European law – produce a derivative or extend the coverage of the linked software licence to your own work. [[1]](https://interoperable-europe.ec.europa.eu/collection/eupl/licence-compatibility-permissivity-reciprocity-and-interoperability)

You are allowed to use this plugin in proprietary and commercial projects, free of charge.

If you distribute modified versions of the code *in this repo*, you must open source those changes.

Feel free to make proprietary projects using NobodyWho, but don't make a proprietary fork of NobodyWho.
