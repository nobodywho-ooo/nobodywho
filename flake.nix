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
      game = pkgs.callPackage ./example-game { inherit nobodywho; };
    in
  {
      packages.default = nobodywho;
      packages.game = game;
      checks.default = game;
      devShells.default = import ./nobodywho/shell.nix { inherit pkgs; };
  });
}
