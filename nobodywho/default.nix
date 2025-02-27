{ pkgs, lib }:

# Techdept: 
#  Currently this system is rebuilding all dependencies for every build. Nothing is cached and
#  the godot and unity package will have to be rebuild whenever there is a change to any of the packages.
# some of this is due to the usage of rustPlatform.buildRustPackage
# we have tried to use crane, but there is a bug in the build.rs of rustbindings (so it requires upstream fix to either llama.cpp, rustbindings or crane)
let
  # Import core first
  core = pkgs.callPackage ./core {};
  
  # # Pass the core package to godot
  # godot = pkgs.callPackage ./godot { 
  #   inherit core;
  # };
  
  # Import unity module
  unity = pkgs.callPackage ./unity { 
    inherit core;
  };
  
in {
  core = core.nobodywho;
  unity = unity.unity_editor;
  # packages = {
  #   # Export each package with the correct name
  #   #godot = godot.packages.godot;
  #   #unity = unity.unity_editor;
  # };
  
  # Merge checks from all modules
  checks = (lib.mapAttrs (name: value: value) unity.checks);
    # (lib.mapAttrs (name: value: value) godot.checks)
    
}
