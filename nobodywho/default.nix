{ pkgs, lib }:

# TODO: fix techdebt: 
#  Currently this system is rebuilding all dependencies for every build. Nothing is cached and
#  the godot and unity package will have to be rebuild whenever there is a change to any of the packages.
#  some of this is due to the usage of rustPlatform.buildRustPackage
#  we have tried to use crane, but there is a bug in the build.rs of rustbindings (so it requires upstream fix to either llama.cpp, rustbindings or crane)
let
  # Import core first
  core = pkgs.callPackage ./core {};
  core-pkg = core.nobodywho;
  
  godot-pkg = pkgs.callPackage ./godot { nobodywho = core-pkg; };
  unity = pkgs.callPackage ./unity { nobodywho = core-pkg; };
  
in {
  core = core-pkg;
  unity-editor = unity.unity-editor;
  godot = godot-pkg.godot;
  
  # TODO: fix techdebt: we can't run unity test through here :/
  checks = (lib.mapAttrs (name: value: value) godot-pkg.checks);
}
