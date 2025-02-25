{
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = (import nixpkgs { system = system; });
      nobodywho = pkgs.callPackage ./nobodywho {};
    in
    { 
      packages = nobodywho.packages;
      checks = nobodywho.checks; 
      devShells.default = import ./nobodywho/shell.nix { inherit pkgs; };
    });
}
