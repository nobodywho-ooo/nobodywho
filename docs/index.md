![NobodyWho Banner](assets/banner.png)


**NobodyWho** is an open-source framework that lets you easily - and without deep technical knowhow - 
run large-language-models completely free and offline.

It has a simple but powerful interface that makes it possible for non , all powered by llama.cpp. 
Because every token is generated on the user’s machine, there are no cloud fees, unpredictable latency, 
and GDPR headaches, while still getting GPU-accelerated throughput via Vulkan or Metal back-ends.



## Made with NobodyWho!
<div class="grid cards" markdown>
- :fontawesome-solid-book: [__Neophyte__](https://dragoonflypj.itch.io/neophyte)
	* Describe your actions and use the items you buy with your words to finish off the enemies.
- :fontawesome-solid-truck: [__The Merchant's Road__](https://svntax.itch.io/the-merchants-road)
	* An LLM-driven text adventure where you guard a merchant in his travels.
- :fontawesome-solid-building: [__Who Farted in the Elevator?__](https://osuika.itch.io/who-farted-in-the-elevator)
	* LLM game where you talk to NPCs about farting in the elevator.
- :fontawesome-solid-calculator: [__Procedural Gungeon__](https://agreene5.itch.io/procedural-gungeon)
	* A procedurally generated 2D shooter that takes place in an infinite dungeon.
- :fontawesome-solid-box: [__The Black Box__](https://profour.itch.io/the-black-box)
	* Half Life inspired short story with a mysterious Black Box.
- :fontawesome-solid-users: [__Ai Argument__](https://blueoctopus.itch.io/ai-argument)
	* A party game where you argue a position before an AI judge.
- :fontawesome-solid-clock: [__The World Will End in 60 Seconds!__](https://coffeepasta.itch.io/the-world-will-end-in-60-seconds)
	* What will you do before it’s all over?
- :fontawesome-solid-rocket: [__The Asteroid__](https://github.com/cesare-montresor/TheAsteroid)
    * A game where you can chat with the crew of a spacestation to figure out what happened in the accident.

</div>


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

Selecting the best model for your use-case is mostly about finding the right trade-off between speed, memory usage and quality of the responses.
Using bigger models will yield better responses, but raise minimum system requirements and slow down generation speed.

### NobodyWho makes Godot crash on Arch Linux / Manjaro
The Godot build currently in the Arch Linux repositories does not work with gdextensions at all.

The solution for Arch users is to install Godot from elsewhere. The binary being distributed from the godotengine.org website works great.
Other distribution methods like nix, flatpak, or building from source also seem to work great.

If anyone knows how to report this issue and to whom, feel free to do so. At this point I have met many Arch Linux users who have this issue.

### NobodyWho fails to load on NixOS
If using a Godot engine from nixpkgs, with NobodyWho binaries from the Godot Asset Library, it will most likely fail to look up dynamic dependencies (libgomp, vulkan-loader, etc).

The reason is that the dynamic library .so files from the Godot Asset Library are compiled for generic Linux, and expect to find them in FHS directories like /lib, which on NixOS will not contain any dynamic libraries.

There are two good solutions for this:

1. The easy way: run the Godot editor using steam-run: `steam-run godot4 --editor`
2. The Nix way: compile NobodyWho using Nix. This repo contains a flake, so it's fairly simple to do (if you have nix with nix-command and flakes enabled): `nix build github:nobodywho-ooo/nobodywho`. Remember to move the dynamic libraries into the right directory afterwards.

### Can I export to HTML5, Android or iOS?

Currently only Linux, MacOS, and Windows are supported platforms.

Mobile exports seem very feasible. See issues [#114](https://github.com/nobodywho-ooo/nobodywho/issues/114), [#66](https://github.com/nobodywho-ooo/nobodywho/issues/66), and [#67](https://github.com/nobodywho-ooo/nobodywho/pull/67) for progress.

Web exports will be a bit trickier to get right. See issue [#111](https://github.com/nobodywho-ooo/nobodywho/issues/111).


## Licensing

There has been some confusion about the licensing terms of this plugin. To clarify:

You are allowed to use this plugin in proprietary and commercial projects, free of charge.

If you distribute modified versions of the code *in this repo*, you must open source those changes.

Feel free to make proprietary games using NobodyWho, but don't make a proprietary fork of NobodyWho.### Can I export to HTML5, Android or iOS?
Currently only Linux, macOS, and Windows are supported platforms.
