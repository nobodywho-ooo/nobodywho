{
  nixConfig = {
    extra-substituters = "https://cache.nixos.org https://hydra.nixos.org https://nobodywho.cachix.org https://nix-community.cachix.org";
    extra-trusted-public-keys = "cache.nixos.org-1:6NCHdD59X431o0gWypbMrAURkbJ16ZPMQFGspcDShjY= hydra.nixos.org-1:CNHJZBh9K4tP3EKF6FkkgeVYsS3ohTl+oS0Qa8bezVs= nobodywho.cachix.org-1:VdcBFxLwfO1L23J973e4UolSnt3QlSZvT1E23+L+9WU= nix-community.cachix.org-1:mB9FSh9qf2dCimDSUo8Zy7bkq5CX+/rkCWyvRCYg3Fs=";
    extra-experimental-features = "nix-command flakes";
  };
  description = "NobodyWho - a godot plugin for NPC dialogue with local LLMs";
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    android-nixpkgs.url = "github:tadfisher/android-nixpkgs";
    crate2nix.url = "github:nix-community/crate2nix";
    crate2nix.inputs.nixpkgs.follows = "nixpkgs";
  };
  outputs =
    {
      nixpkgs,
      flake-utils,
      android-nixpkgs,
      crate2nix,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = (
          import nixpkgs {
            inherit system;
            config = {
              allowUnfreePredicate =
                pkg:
                builtins.elem (pkgs.lib.getName pkg) [
                  "unityhub"
                  "corefonts"
                ]; # allow unfree unityhub
              android_sdk.accept_license = true;
            };
          }
        );

        nobodywho-godot = pkgs.callPackage ./nobodywho/godot { };

        nobodywho-python = pkgs.callPackage ./nobodywho/python { };
      in
      {
        # default: the godot gdextension dynamic lib
        packages.default = nobodywho-godot.nobodywho-godot;

        # checks
        checks.default = pkgs.callPackage ./nobodywho/flutter/example_app { };
        checks.flutter_example_app = pkgs.callPackage ./nobodywho/flutter/example_app { };
        checks.build-godot = nobodywho-godot.nobodywho-godot;
        checks.godot-integration-test = nobodywho-godot.run-integration-test;
        checks.nobodywho-python = nobodywho-python;

        # the Everything devshell
        devShells.default = pkgs.callPackage ./nobodywho/shell.nix { inherit android-nixpkgs; };

        # godot stuff
        packages.nobodywho-godot = nobodywho-godot.nobodywho-godot;

        # flutter stuff
        packages.flutter_example_app = pkgs.callPackage ./nobodywho/flutter/example_app { };
        packages.flutter_rust = pkgs.callPackage ./nobodywho/flutter/rust { inherit crate2nix; };

        # python stuff
        packages.nobodywho-python = nobodywho-python;
        devShells.nobodywho-python = pkgs.mkShell {
          # a devshell that includes the built python package
          # useful for testing local changes in repl or pytest
          packages = [
            (nobodywho-python.override { doCheck = false; })
            pkgs.python3Packages.pytest
            pkgs.python3Packages.pytest-asyncio
          ];
        };

        devShells.android = pkgs.callPackage ./nobodywho/android.nix { inherit android-nixpkgs; };
      }
    );
}
