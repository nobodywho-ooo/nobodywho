## Frequently Asked Questions

### Where do I find good models to use?

New language models are coming out at a breakneck pace. If you search the web for "best language models for roleplay" or something similar, you'll probably find results that are several months or years old. You want to use something newer.

Selecting the best model for your use-case is mostly about finding the right trade-off between speed, memory usage and quality of the responses.
Using bigger models will yield better responses, but raise minimum system requirements and slow down generation speed.

Have a look at our [model selection guide](../model-selection.md) for more in-depth recommendations.


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