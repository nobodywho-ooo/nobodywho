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
         config.allowUnfreePredicate = pkg: builtins.elem (pkgs.lib.getName pkg) [
             "steam-run"
             "steam-original"
             "steam-unwrapped"
         ];
      });
      nobodywho = pkgs.callPackage ./nobodywho {};
      integration-test = pkgs.callPackage ./example-game { inherit nobodywho; };
      run-integration-test = pkgs.runCommand "checkgame" { nativeBuildInputs = with pkgs; [ mesa.drivers ]; } ''
        cd ${integration-test}
        ${pkgs.steam-run}/bin/steam-run ./game --headless
        touch $out
      '';
    in
  {
      packages.default = nobodywho;
      packages.integration-test = integration-test;
      checks.default = run-integration-test;
      devShells.default = import ./nobodywho/shell.nix { inherit pkgs; };
  });
}
