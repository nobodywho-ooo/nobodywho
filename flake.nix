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
        config.allowUnfree = true;  # Required for Unity
      });
      nobodywho = pkgs.callPackage ./nobodywho {};
    in
    { 
      packages = {
        nobodywho = nobodywho.core;
        unity = nobodywho.unity;
        # godot = nobodywho.godot;
        
      };
      checks = {
        unity-test =nobodywho.checks.unity-test;
      };
      devShells = {
        default = import ./nobodywho/shell.nix { inherit pkgs; };
      };
    });
}
