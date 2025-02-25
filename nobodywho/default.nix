{ pkgs, lib }:


# Techdept: 
#  Currently this sytem uis rebuilding all dependencies for every build. Nothing is cached and
#  the godot and unity package will have to be rebuild whenever there is a change to any of the packages.
# some of this is due to the usage of rustPlatform.buildRustPackage
# we have tried to use crane, but there is a bug in the build.rs of rustbindings (so it requires upstream fix to either llama.cpp, rustbindings or crane)
let
  nobodywho = pkgs.callPackage ./core { };
  godot = pkgs.callPackage ./godot { inherit nobodywho; };

in {
  packages = {
    default = nobodywho;
    godot = godot.packages.default;
  };
  
  checks = lib.mapAttrs (name: value: value) godot.checks;
}
