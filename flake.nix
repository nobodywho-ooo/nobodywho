{
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = (import nixpkgs { 
        system = system;
        config = {
          allowUnfreePredicate = pkg: builtins.elem (pkgs.lib.getName pkg) [
            "unityhub" # allow unfree unity
          ];
        };
      });
      nobodywho = pkgs.callPackage ./nobodywho {};
    in
    { 
      packages = {
        default = nobodywho.core;
        nobodywho = nobodywho.core;
        unity = nobodywho.unity-editor;
        godot = nobodywho.godot;
        
      };
      checks = {
        default = nobodywho.checks.godot-integration-test;
      };
      devShells = {
        default = import ./nobodywho/shell.nix { inherit pkgs; };
      };
    });
}
