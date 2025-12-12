![Nobody Who](./assets/banner.png)

[![Discord](https://img.shields.io/discord/1308812521456799765?logo=discord&style=flat-square)](https://discord.gg/qhaMc2qCYB)
[![Matrix](https://img.shields.io/badge/Matrix-000?logo=matrix&logoColor=fff)](https://matrix.to/#/#nobodywho:matrix.org)
[![Mastodon](https://img.shields.io/badge/Mastodon-6364FF?logo=mastodon&logoColor=fff&style=flat-square)](https://mastodon.gamedev.place/@nobodywho)
[![Godot Engine](https://img.shields.io/badge/Godot-%23FFFFFF.svg?logo=godot-engine&style=flat-square)](https://godotengine.org/asset-library/asset/2886)
[![Contributor Covenant](https://img.shields.io/badge/Contributor%20Covenant-2.1-4baaaa.svg?style=flat-square)](CODE_OF_CONDUCT.md) 
[![Docs](https://img.shields.io/badge/Read%20the%20Docs-8CA1AF?logo=readthedocs&logoColor=fff)](https://nobodywho-ooo.github.io/nobodywho/)


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


## How to Install

### Python

Just do as usual:

```sh
pip install nobodywho
```

### Godot 

You can install it from inside the Godot editor: In Godot 4.4+, go to AssetLib and search for "NobodyWho".

...or you can grab a specific version from our [github releases page.](https://github.com/nobodywho-ooo/nobodywho/releases) You can install these zip files by going to the "AssetLib" tab in Godot and selecting "Import".

Make sure that the ignore asset root option is set in the import dialogue.

## Documentation

[The documentation](https://nobodywho-ooo.github.io/nobodywho/) has everything you might want to know: https://nobodywho-ooo.github.io/nobodywho/


## How to Help 

* ⭐ Star the repo and spread the word about NobodyWho!
* Join our [Discord](https://discord.gg/qhaMc2qCYB) or [Matrix](https://matrix.to/#/#nobodywho:matrix.org) communities
* Found a bug? Open an issue!
* Submit your own PR - contributions welcome
* Help improve docs or write tutorials

## Frequently Asked Questions

### Where do I find good models to use?

New language models are coming out at a breakneck pace. If you search the web for "best language models for roleplay" or something similar, you'll probably find results that are several months or years old. You want to use something newer.

Selecting the best model for your use-case is mostly about finding the right trade-off between speed, memory usage and quality of the responses.
Using bigger models will yield better responses, but raise minimum system requirements and slow down generation speed.

Have a look at our [model selection guide](https://nobodywho-ooo.github.io/nobodywho/model-selection/) for more in-depth recommendations.


### Once I export my Godot project, it can no longer find the model file.

Exports are a bit weird for now: Llama.cpp expects a path to a GGUF file on your filesystem, while Godot really wants to package everything in one big .pck file.

The solution (for now) is to manually copy your chosen GGUF file into the export directory (the folder with your exported game executable).

If you're exporting for Android, you can't reliably pass a `res://` path to the model node. The best workaround is to use `user://` instead.
If your model is sufficiently small, you might get away with copying it from `res://` into `user://`. If using double the storage isn't acceptable, consider downloading it at runtime, or find some other way of distributing your model as a file.

We're looking into solutions for including this file automatically.


### NobodyWho-Godot makes Godot crash on Arch Linux / Manjaro

The Godot build currently in the Arch linux repositories does not work with gdextensions at all.

The solution for Arch users is to install godot from elsewhere. The binary being distributed from the godotengine.org website works great.
Other distribution methods like nix, flatpak, or building from source also seems to work great.

If anyone knows how to report this issue and to whom, feel free to do so. At this point I have met many Arch linux users who have this issue.


### NobodyWho-Godot fails to load on NixOS

If using a Godot engine from nixpkgs, with NobodyWho binaries from the Godot Asset Library. It will most likely fail to look up dynamic dependencies (libgomp, vulkan-loader, etc).

The reason is that the dynamic library .so files from the Godot Asset Library are compiled for generic linux, and expect to find them in FHS directories like /lib, which on NixOS will not contain any dynamic libraries.

There are two good solutions for this:

1. The easy way: run the godot editor using steam-run: `steam-run godot4 --editor`
2. The Nix way: compile NobodyWho using Nix. This repo contains a flake, so it's fairly simple to do (if you have nix with nix-command and flakes enabled): `nix build github:nobodywho-ooo/nobodywho`. Remember to move the dynamic libraries into the right directory afterwards.


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
