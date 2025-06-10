![Nobody Who](./assets/banner.png)

[![Discord](https://img.shields.io/discord/1308812521456799765?logo=discord&style=flat-square)](https://discord.gg/qhaMc2qCYB)
[![Matrix](https://img.shields.io/badge/Matrix-000?logo=matrix&logoColor=fff)](https://matrix.to/#/#nobodywho:matrix.org)
[![Mastodon](https://img.shields.io/badge/Mastodon-6364FF?logo=mastodon&logoColor=fff&style=flat-square)](https://mastodon.gamedev.place/@nobodywho)
[![Godot Engine](https://img.shields.io/badge/Godot-%23FFFFFF.svg?logo=godot-engine&style=flat-square)](https://godotengine.org/asset-library/asset/2886)
![GitHub Sponsors](https://img.shields.io/github/sponsors/nobodywho-ooo?style=flat-square)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg?style=flat-square)](CODE_OF_CONDUCT.md) 


NobodyWho is a plugin that lets you interact with local LLMs, we currently support Godot and Unity, with even more plugins on their way.


## At a Glance

* 🏃 Run LLM-driven characters locally without internet
* ⚡ Super fast inference on GPU powered by Vulkan or Metal
* 🔧 Easy setup - just two nodes to get started
* 🎯 Perfect for games, interactive stories, and NPCs
* 💻 Cross-platform: Windows, Linux, macOS
* 🦙 Powered by the wonderful [llama.cpp](https://github.com/ggml-org/llama.cpp)

## Demo video

Small demo of a use-case. This video was recorded in real time on a laptop, to give you an idea of performance.

The code for this showcase is in the [demo-game](./demo-game) folder of this repo. It amounts to about 100 lines of code in a single file, most of it being UI stuff.

![](./assets/foobars-potionshop.gif)

## How to Install


### Godot 

You can install it from inside the Godot editor: In Godot 4.4+, go to AssetLib and search for "NobodyWho".

...or you can grab a specific version from our [github releases page.](https://github.com/nobodywho-ooo/nobodywho/releases) You can install these zip files by going to the "AssetLib" tab in Godot and selecting "Import".

Make sure that the ignore asset root option is set in the import dialogue.

### Unity

You can install it from [github releases page.](https://github.com/nobodywho-ooo/nobodywho/releases) while we are still pending acceptance on the unity assets store.
When you download the unity package file, make sure to follow https://docs.unity3d.com/Manual/AssetPackagesImport.html to import the asset. 
Basically `Assets > Import Package > Custom Package`

To see and play around with the samples you might need to right click the plugin and then: `> View in Package Manager > Click on NobodyWho > Samples > Install`. This should set up all the dependencies correctly.
From there you can also check the documentaiton.

## How to Help 

* ⭐ Star the repo and spread the word about NobodyWho!
* Join our [Discord](https://discord.gg/qhaMc2qCYB) or [Matrix](https://matrix.to/#/#nobodywho:matrix.org) communities
* Found a bug? Open an issue!
* Submit your own PR - contributions welcome
* 💝 [Become a sponsor](https://github.com/sponsors/nobodywho-ooo) to support development
* Help improve docs or write tutorials


## Getting started

The plugin does not include a large language model (LLM). You need to provide an LLM in the GGUF file format. A good place to start is something like [Gemma 2 2B](https://huggingface.co/bartowski/gemma-2-2b-it-GGUF/resolve/main/gemma-2-2b-it-Q4_K_M.gguf)

Once you have a GGUF model file, you can add a `NobodyWhoModel` node to your Godot scene. On this node, set the model file to the GGUF model you just downloaded.

`NobodyWhoModel` contains the weights of the model. The model takes up a lot of RAM, and can take a little while to initialize, so if you plan on having several characters/conversations, it's a big advantage to point to the same `NobodyWhoModel` node.

Now you can add a `NobodyWhoChat` node to your scene. From the node inspector, set the "Model Node" field, to show this chat node where to find the `NobodyWhoModel`.
Also in the inspector, you can provide a prompt, which gives the LLM instructions on how to carry out the chat.

Now you can add a script to the `NobodyWhoChat` node, to provide your chat interaction.

`NobodyWhoChat` uses this programming interface:

- `say(text: String)`: a function that can be used to send text from the user to the LLM.
- `response_updated(token: String)`: a signal that is emitted every time the LLM produces more text. Contains roughly one word per invocation.
- `response_finished(response: String)`: a signal which indicates that the LLM is done speaking.
- `start_worker()`: a function that starts the LLM worker. The LLM needs a few seconds to get ready before chatting, so you may want to call this ahead of time.


## Example `NobodyWhoChat` script

```gdscript
extends NobodyWhoChat

func _ready():
	# configure node
	model_node = get_node("../ChatModel")
	system_prompt = "You are an evil wizard. Always try to curse anyone who talks to you."

	# say something
	say("Hi there! Who are you?")

	# wait for the response
	var response = await response_finished
	print("Got response: " + response)

    # in this example we just use the `response_finished` signal to get the complete response
    # in real-world-use you definitely want to connect `response_updated`, which gives one word at a time
    # the whole interaction feels *much* smoother if you stream the response out word-by-word.
```


## Example `NobodyWhoEmbedding` script

```gdscript
extends NobodyWhoEmbedding

func _ready():
    # configure node
    self.model_node = get_node("../EmbeddingModel")

    # generate some embeddings
    embed("The dragon is on the hill.")
    var dragon_hill_embd = await self.embedding_finished

    embed("The dragon is hungry for humans.")
    var dragon_hungry_embd = await self.embedding_finished

    embed("This doesn't matter.")
    var irrelevant_embd = await self.embedding_finished

    # test similarity,
    # here we show that two embeddings will have high similarity, if they mean similar things
    var low_similarity = cosine_similarity(irrelevant_embd, dragon_hill_embd)
    var high_similarity = cosine_similarity(dragon_hill_embd, dragon_hungry_embd) 
    assert(low_similarity < high_similarity)
```

## Frequently Asked Questions

### Once I export my Godot project, it can no longer find the model file.

Exports are a bit weird for now: Llama.cpp expects a path to a GGUF file on your filesystem, while Godot really wants to package everything in one big .pck file.

The solution (for now) is to manually copy your chosen GGUF file into the export directory (the folder with your exported game executable).

We're looking into solutions for including this file automatically.

### Where do I find good models to use?

New language models are coming out at a breakneck pace. If you search the web for "best language models for roleplay" or something similar, you'll probably find results that are several months or years old. You want to use something newer.

We recommend checking leaderboards like [The GPU-Poor LLM Gladiator Arena](https://huggingface.co/spaces/k-mktr/gpu-poor-llm-arena), or [OpenRouter's Roleplay Rankings](https://openrouter.ai/rankings/roleplay).
Once you select a model, remember that you need a quantization of it in the GGUF format.
The huggingface user [bartowski](https://huggingface.co/bartowski) regularly uploads GGUF quantizations for *a lot* of new models.

Selecting the best model for your usecase is mostly about finding the right tradeoff between speed, memory usage and quality of the responses.
Using bigger models general will yield better responses, but raise minimum system requirements and slow down generation speed.

### NobodyWho makes Godot crash on Arch Linux / Manjaro

The Godot build currently in the Arch linux repositories does not work with gdextensions at all.

The solution for Arch users is to install godot from elsewhere. The binary being distributed from the godotengine.org website works great.
Other distribution methods like nix, flatpak, or building from source also seems to work great.

If anyone knows how to report this issue and to whom, feel free to do so. At this point I have met many Arch linux users who have this issue.

### NobodyWho fails to load on NixOS

If using a Godot engine from nixpkgs, with NobodyWho binaries from the Godot Asset Library. It will most likely fail to look up dynamic dependencies (libgomp, vulkan-loader, etc).

The reason is that the dynamic library .so files from the Godot Asset Library are compiled for generic linux, and expect to find them in FHS directories like /lib, which on NixOS will not contain any dynamic libraries.

There are two good solutions for this:

1. The easy way: run the godot editor using steam-run: `steam-run godot4 --editor`
2. The Nix way: compile NobodyWho using Nix. This repo contains a flake, so it's faily simple to do (if you have nix with nix-command and flakes enabled): `nix build github:nobodywho-ooo/nobodywho`. Remember to move the dynamic libraries into the right directory afterwards.

### Can I export to HTML5, Android or iOS?

Currently only Linux, MacOS, and Windows are supported platforms.

Mobile exports seem very feasible. See issues [#114](https://github.com/nobodywho-ooo/nobodywho/issues/114), [#66](https://github.com/nobodywho-ooo/nobodywho/issues/66), and [#67](https://github.com/nobodywho-ooo/nobodywho/pull/67) for progress.

Web exports will be a bit trickier to get right. See issue [#111](https://github.com/nobodywho-ooo/nobodywho/issues/111).


## Licensing

There has been some confusion about the licensing terms of this plugin. To clarify:

You are allowed to use this plugin in proprietary and commercial projects, free of charge.

If you distribute modified versions of the code *in this repo*, you must open source those changes.

Feel free to make proprietary games using NobodyWho, but don't make a proprietary fork of NobodyWho.


# Featured Examples

* [Neophyte](https://dragoonflypj.itch.io/neophyte)
	* Describe your actions and use the items you buy with your words to finish off the enemies.
* [The Merchant's Road](https://svntax.itch.io/the-merchants-road)
	* An LLM-driven text adventure where you guard a merchant in his travels.
* [Who Farted in the Eleveator?](https://osuika.itch.io/who-farted-in-the-elevator)
	* LLM game where you talk to NPCs about farting in the elevator.
* [Procedural](https://agreene5.itch.io/procedural-gungeon)
	* A procedurally generated 2D shooter that takes place in an infinite dungeon.
* [The Black Box](https://profour.itch.io/the-black-box)
	* Half Life inspired short story with a mysterious Black Box.
* [Ai rgument](https://blueoctopus.itch.io/ai-rgument)
	* A party game where you argue a position before an AI judge.
* [The World Will End in 60 Seconds!](https://coffeepasta.itch.io/the-world-will-end-in-60-seconds)
	* What will you do before it’s all over?
* [Stonecot Prototype](https://windarthouse.itch.io/stonecot-prototype)
	* Stonecot Prototype is a stripped-down, experimental build of Mythara Chronicles, showcasing AI-driven party interactions and a prototype main quest. 
* [The Asteroid](https://github.com/cesare-montresor/TheAsteroid)
    * A game where you can chat with the crew of a spacestation to figure out what happened in the accident.
