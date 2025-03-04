{ pkgs, lib }:

# Techdept: 
#  Currently this system is rebuilding all dependencies for every build. Nothing is cached and
#  the godot and unity package will have to be rebuild whenever there is a change to any of the packages.
#  some of this is due to the usage of rustPlatform.buildRustPackage
#  we have tried to use crane, but there is a bug in the build.rs of rustbindings (so it requires upstream fix to either llama.cpp, rustbindings or crane)
let
  # Import core first
  core = pkgs.callPackage ./core {};
  nobodywho = core.nobodywho;
  
  godot = pkgs.callPackage ./godot { inherit nobodywho; };
  unity = pkgs.callPackage ./unity { inherit nobodywho; };
  
in {
  core = nobodywho;
  unity-editor = unity.unity-editor;
  godot = godot.godot;
  
  # Merge checks from all modules
  checks = (lib.mapAttrs (name: value: value) unity.checks);
    # (lib.mapAttrs (name: value: value) godot.checks)
    
}
