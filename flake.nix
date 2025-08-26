{
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    android-nixpkgs.url = "github:tadfisher/android-nixpkgs";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      android-nixpkgs,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (
          import nixpkgs {
            inherit system;
            config.allowUnfreePredicate = pkg: builtins.elem (pkgs.lib.getName pkg) [ "unityhub" "corefonts" ]; # allow unfree unityhub
          }
        );

        nobodywho-godot = pkgs.callPackage ./nobodywho/godot { };
      in
      {
        packages.default = nobodywho-godot.nobodywho-godot;
        checks.default = nobodywho-godot.run-integration-test;
        devShells.default = pkgs.callPackage ./nobodywho/shell.nix { inherit android-nixpkgs; };
      }
    );
}
