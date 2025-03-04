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
          allowUnfree = true;  # Required for Unity
          permittedInsecurePackages = [
            "freeimage-unstable-2021-11-01"
            "openssl-1.1.1w"
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
        unity-test =nobodywho.checks.unity-test;
      };
      devShells = {
        default = import ./nobodywho/shell.nix { inherit pkgs; };
      };
    });
}
